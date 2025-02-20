mod logger;
use crate::logger::Logger;
use wasm_bindgen::prelude::*;
use futures::stream::StreamExt;
use web_sys::*;
use wasm_bindgen_futures::JsFuture;
use futures::channel:: mpsc;
use gl_matrix::common::*;
use gl_matrix::{vec3,mat4,quat};
use std::rc::Rc;
use std::cell::RefCell;

pub struct GlProgram{
    gl: WebGl2RenderingContext,
    program: WebGlProgram,
}
impl From<GlProgram> for JsValue{
    fn from(gl_program: GlProgram)->Self{
        JsValue::from(gl_program.program)
    }
}
impl wasm_bindgen::describe::WasmDescribe for GlProgram{
    fn describe() {
        <JsValue as wasm_bindgen::describe::WasmDescribe>::describe();
    }
}

#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue>{

    // ブラウザのオブジェクトを取得
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let performance = window.performance().expect("should have a performance on window");
    let body = document.body().expect("document doesn't have body.");
    let button = document.create_element("button")?.dyn_into::<HtmlButtonElement>()?;

    // XRのスタートボタンの押し待ち用mpsc
    let (mut button_tx,mut button_rx) = mpsc::channel::<()>(32);

    // ボタンの設定
    button.set_inner_text("Start WebXR");
    let button_clone = button.clone();
    let onclick_func = Closure::wrap(Box::new(move ||{
        button_clone.set_inner_text("WebXR is starting...");
        // 送れるまでbutton_txを送る
        loop{
            let Err(_) = button_tx.try_send(()) else{
                break;
            };
        }
    })as Box<dyn FnMut()>);
    button.set_onclick(Some(onclick_func.as_ref().unchecked_ref::<js_sys::Function>()));
    let _ = body.append_child(&button)?;

    button_rx.next().await;
    
    // XRSystemを取得して、環境でwebXRが実行可能であるか確認
    let xrsystem = window.navigator().xr();
    let Some(xrsession) = webxr_available(&xrsystem,&document).await? else{
        console::log_1(&"WebXR is not available".into());
        display_error_page(&document, "WebXR is not available").await?;
        return Ok(());
    };

    // webgl2のコンテキストを作成し、webXRに対応させる
    let gl = create_webgl2_context(&document).await?;
    console::log_1(&"created webgl2 context".into());
    wasm_bindgen_futures::JsFuture::from(gl.make_xr_compatible()).await?;
    console::log_1(&"made webgl2 context xr compatible".into());
    let gl_program = ready_webgl2_context(&window, &document,gl).await?;
    console::log_1(&"created webgl2 context".into());
    
    create_webxr_session(xrsession, gl_program.gl, gl_program.program,performance).await;
    Ok(())
}

#[wasm_bindgen]
pub async fn create_webxr_session(xrsession: XrSession, gl: WebGl2RenderingContext, program: WebGlProgram, performance: Performance){
    let render_state = XrRenderStateInit::new();
    let Ok(webgl_layer) = XrWebGlLayer::new_with_web_gl2_rendering_context(&xrsession, &gl) else{
        console::log_1(&"[Error] Could not create WebGlLayer".into());
        return;
    };
    
    render_state.set_base_layer(Some(&webgl_layer));
    xrsession.update_render_state_with_state(&render_state);
    let Ok(reference_space_js) = JsFuture::from(xrsession.request_reference_space(XrReferenceSpaceType::Local)).await else{
        console::log_1(&"[Error] Could not get reference space".into());
        return;
    };

    if XrReferenceSpace::instanceof(&reference_space_js){
        let reference_space = XrReferenceSpace::unchecked_from_js(reference_space_js);
        let animation_loop = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64,XrFrame)>>));
        let mut fps_tracker = Logger::new(performance);
        //初期状態はNone
        //RefCellはClosureを後で自分自身を参照できるようにするためのラッパー
        //Rcは複数の所有者を持つためのスマートポインタ

        let animation_loop_clone = Rc::clone(&animation_loop);
        let session_clone = xrsession.clone();
        *animation_loop.borrow_mut() = Some(Closure::wrap(Box::new(move |time: f64, frame: XrFrame|{
            fps_tracker.track_frame();
            fps_tracker.log_fps();
            fps_tracker.log_memory_usage();
            render_frame(time, &frame, &reference_space, &session_clone, &gl, &program);
            session_clone.request_animation_frame(animation_loop_clone.borrow().as_ref().unwrap().as_ref().unchecked_ref::<js_sys::Function>());
        }) as Box<dyn FnMut(f64,XrFrame)>));

        //最初のアニメーションフレームをリクエスト
        let _animation_frame_request_id = xrsession.request_animation_frame(animation_loop.borrow().as_ref().unwrap().as_ref().unchecked_ref::<js_sys::Function>());
    }
}

#[wasm_bindgen]
pub fn render_frame(_time: f64, frame: &XrFrame, reference_space: &XrReferenceSpace, _session: &XrSession, gl: &WebGl2RenderingContext, program: &web_sys::WebGlProgram){
    let pose = frame.get_viewer_pose(reference_space);
    if let Some(pose) = pose{
        let gl_layer = frame.session().render_state().base_layer().unwrap();
        gl.bind_framebuffer(WebGl2RenderingContext::FRAMEBUFFER, gl_layer.framebuffer().as_ref());
        console::log_1(&"bind framebuffer".into());

        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear_depth(1.0);
        gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT);
        console::log_1(&"gl clear".into());

        for view in pose.views(){
            let xrview = view.dyn_into::<XrView>().unwrap();
            let viewport = gl_layer.get_viewport(&xrview).unwrap();
            gl.viewport(viewport.x(), viewport.y(), viewport.width(), viewport.height());
            console::log_1(&"setting viewport for eye".into());
            let canvas = gl.canvas().unwrap().dyn_into::<HtmlCanvasElement>().unwrap();
            canvas.set_width(viewport.width() as u32 * pose.views().length());
            canvas.set_height(viewport.height() as u32);
            render_scene(gl, &xrview, program);
        }
    }
}

#[wasm_bindgen]
pub fn render_scene(gl: &WebGl2RenderingContext, view: &XrView, program: &web_sys::WebGlProgram){
    let mut scale = mat4::create();
    let scale_clone = scale;
    mat4::scale(&mut scale,&scale_clone, &[1.0,1.0,1.0]);

    let mut rotation = mat4::create();
    let rotation_clone = rotation;
    mat4::rotate_z(&mut rotation, &rotation_clone, PI/8.0);

    let mut translation = mat4::create();
    let translation_clone = translation;
    mat4::translate(&mut translation, &translation_clone, &[40.0,0.0,-20.0]);

    let mut model = mat4::create();
    let model_clone = model;
    mat4::multiply(&mut model, &model_clone, &translation);
    mat4::multiply(&mut model, &model_clone, &rotation);
    mat4::multiply(&mut model, &model_clone, &scale);

    let view_position = view.transform().position();
    let camera_position = vec3::from_values(view_position.x() as f32, view_position.y() as f32, view_position.z() as f32);
    let view_direction = view.transform().orientation();
    let view_quat = quat::from_values(view_direction.x() as f32, view_direction.y() as f32, view_direction.z() as f32, view_direction.w() as f32);
    let mut view_matrix = mat4::create();
    mat4::from_rotation_translation(&mut view_matrix, &view_quat, &camera_position);

    let projection_from_view = view.projection_matrix();
    let projection:Mat4 = projection_from_view.try_into().unwrap();

    let model_location = gl.get_uniform_location(program, "model");
    let view_location = gl.get_uniform_location(program, "view");
    let projection_location = gl.get_uniform_location(program, "projection");
    gl.uniform_matrix4fv_with_f32_array(model_location.as_ref(), false, &model);
    gl.uniform_matrix4fv_with_f32_array(view_location.as_ref(), false, &view_matrix);
    gl.uniform_matrix4fv_with_f32_array(projection_location.as_ref(), false, &projection);

    gl.draw_elements_with_i32(WebGl2RenderingContext::TRIANGLES, 36, WebGl2RenderingContext::UNSIGNED_SHORT, 0);
}

#[derive(Debug,Clone)]
pub enum ShaderVariant{
    Vertex(String),
    Fragment(String),
}

pub struct Shader{
    vertex_shader: Option<String>,
    fragment_shader: Option<String>,
}


#[wasm_bindgen]
pub async fn ready_webgl2_context(window: &Window, document: &Document, gl: WebGl2RenderingContext)->Result<GlProgram ,JsValue>{
    let (shader_tx, mut shader_rx) = mpsc::channel::<ShaderVariant>(32);

    let shaders = async move{
        let mut is_vertex_received = false;
        let mut is_fragment_received = false;
        let mut shader = Shader{
            vertex_shader: None,
            fragment_shader: None,
        };

        while let Some(message) = shader_rx.next().await{
            match message{
                ShaderVariant::Vertex(vertex_shader)=>{
                    is_vertex_received = true;
                    shader.vertex_shader = Some(vertex_shader);
                    console::log_1(&"Received Vertex Shader".into());
                },
                ShaderVariant::Fragment(fragment_shader)=>{
                    is_fragment_received = true;
                    shader.fragment_shader = Some(fragment_shader);
                    console::log_1(&"Received Fragment Shader".into());
                },
            }
            if is_vertex_received && is_fragment_received{
                break;
            }
        }
        shader
    };

    console::log_1(&"Starting create_webgl2_context".into());
    console::log_1(&"Start Shader Fetch Task".into());

    let window_clone = window.clone();
    let document_clone = document.clone();
    let mut vertex_tx = shader_tx.clone();

    wasm_bindgen_futures::spawn_local(async move{
        let Ok(vertex_shader) = fetch_shader(window_clone, "../shader/vertex_shader.glsl").await else{
            console::log_1(&"[Error] Could not fetch vertex shader".into());
            let _ = display_error_page(&document_clone,"Could not fetch vertex shader").await;
            return;
        };
        loop{
            let Err(_) = vertex_tx.try_send(ShaderVariant::Vertex(vertex_shader.clone())) else{
                break;
            };
        }
    });

    let window_clone = window.clone();
    let document_clone = document.clone();
    let mut fragment_tx = shader_tx.clone();

    wasm_bindgen_futures::spawn_local(async move{
        let Ok(fragment_shader) = fetch_shader(window_clone, "../shader/fragment_shader.glsl").await else{
            console::log_1(&"[Error] Could not fetch fragment shader".into());
            let _ = display_error_page(&document_clone,"Could not fetch fragment shader").await;
            return;
        };
        loop{
            let Err(_) = fragment_tx.try_send(ShaderVariant::Fragment(fragment_shader.clone())) else{
                break;
            };
        }
    });

    let shader = shaders.await;
    let program = compile_shader(&gl, &shader.vertex_shader.unwrap(), &shader.fragment_shader.unwrap()).await?;

    const VERTEX_SIZE: i32 = 3;
    const COLOR_SIZE: i32 = 4;

    const FLOAT32_BYTES_PER_ELEMENT: i32 = 4;
    const STRIDE: i32 = (VERTEX_SIZE + COLOR_SIZE) * FLOAT32_BYTES_PER_ELEMENT;
    const POSITION_OFFSET: i32 = 0;
    const COLOR_OFFSET: i32 = VERTEX_SIZE * FLOAT32_BYTES_PER_ELEMENT;

    let vertices:[f32;56] = [
        0.0, 0.5, -0.5,  // 座標
        1.0, 1.0, 1.0, 1.0,      // 色
        0.0, 0.5, 0.0,  // 座標
        0.0, 1.0, 1.0, 1.0,      // 色
        0.5, 0.5, -0.5,  // 座標
        1.0, 0.0, 1.0, 1.0,      // 色
        0.5, 0.5, 0.0,  // 座標
        0.0, 1.0, 0.0, 1.0,      // 色
        0.0, 0.0, -0.5,  // 座標
        1.0, 1.0, 0.0, 1.0,      // 色
        0.0, 0.0, 0.0,  // 座標
        1.0, 0.0, 1.0, 1.0,      // 色
        0.0, 0.5, -0.5,  // 座標
        1.0, 0.0, 0.0, 1.0,      // 色
        0.5, 0.0, 0.0,  // 座標
        0.0, 1.0, 1.0, 1.0,      // 色
    ];
    let indices: [u16; 36] =[
        0, 1, 2,
        1, 3, 2,
        1, 5, 3,
        3, 5, 7,
        3, 7, 2,
        2, 7, 6,
        0, 2, 6,
        0, 6, 4,
        0, 5, 1,
        0, 4, 5,
        7, 5, 6,
        5, 4, 6,
    ];

    let interleaved_buffer = create_f32_buffer(WebGl2RenderingContext::ARRAY_BUFFER, &vertices, &gl).await?;
    let index_buffer = create_u16_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, &indices, &gl).await?;

    let vertex_attrib_location = gl.get_attrib_location(&program, "vertex_position");
    let color_attrib_location = gl.get_attrib_location(&program, "color");

    gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&interleaved_buffer));
    gl.enable_vertex_attrib_array(vertex_attrib_location as u32);
    gl.vertex_attrib_pointer_with_i32(vertex_attrib_location as u32, VERTEX_SIZE, WebGl2RenderingContext::FLOAT, false, STRIDE, POSITION_OFFSET);
    gl.enable_vertex_attrib_array(color_attrib_location as u32);
    gl.vertex_attrib_pointer_with_i32(color_attrib_location as u32, COLOR_SIZE, WebGl2RenderingContext::FLOAT, false, STRIDE, COLOR_OFFSET);

    gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&index_buffer));
    gl.enable(WebGl2RenderingContext::DEPTH_TEST);
    gl.enable(WebGl2RenderingContext::CULL_FACE);

    let gl_program = GlProgram{gl, program};
    Ok(gl_program)
}

#[wasm_bindgen]
pub async fn fetch_shader(window: Window, shader_path: &str)->Result<String,JsValue>{
    let shader_response = JsFuture::from(
        window.fetch_with_str(shader_path),
    ).await?;

    let shader_response = shader_response.dyn_into::<Response>()?;

    let shader_text = JsFuture::from(
        shader_response.text()?
    ).await?;

    let Some(shader_text) = shader_text.as_string() else{
        console::log_1(&"[Error] Shader was none".into());
        return Err(JsValue::null());
    };

    Ok(shader_text)
}

#[wasm_bindgen]
pub async fn compile_shader(gl: &WebGl2RenderingContext, vertex: &str, fragment: &str)->Result<WebGlProgram,JsValue>{
    let vertex_shader = gl.create_shader(WebGl2RenderingContext::VERTEX_SHADER).unwrap();
    gl.shader_source(&vertex_shader, vertex);
    gl.compile_shader(&vertex_shader);

    let fragment_shader = gl.create_shader(WebGl2RenderingContext::FRAGMENT_SHADER).unwrap();
    gl.shader_source(&fragment_shader, fragment);
    gl.compile_shader(&fragment_shader);

    let Some(program) = gl.create_program() else{
        console::log_1(&"[Error] Could not create program".into());
        return Err(JsValue::null());
    };
    gl.attach_shader(&program, &vertex_shader);
    gl.attach_shader(&program, &fragment_shader);
    gl.link_program(&program);

    gl.use_program(Some(&program));

    Ok(program)
}

#[wasm_bindgen]
pub async fn create_f32_buffer(buffer_type: u32, typed_data_array: &[f32], gl: &WebGl2RenderingContext) -> Result<web_sys::WebGlBuffer, JsValue>{
    let buffer = gl.create_buffer().unwrap();
    gl.bind_buffer(buffer_type, Some(&buffer));
    let array = js_sys::Float32Array::from(typed_data_array);
    gl.buffer_data_with_array_buffer_view(buffer_type, &array, WebGl2RenderingContext::STATIC_DRAW);

    // バッファのバインドを解除
    gl.bind_buffer(buffer_type, None);

    Ok(buffer)
}

#[wasm_bindgen]
pub async fn create_u16_buffer(buffer_type: u32, typed_data_array: &[u16], gl: &WebGl2RenderingContext) -> Result<web_sys::WebGlBuffer, JsValue>{
    let buffer = gl.create_buffer().unwrap();
    gl.bind_buffer(buffer_type, Some(&buffer));
    let array = js_sys::Uint16Array::from(typed_data_array);
    gl.buffer_data_with_array_buffer_view(buffer_type, &array, WebGl2RenderingContext::STATIC_DRAW);

    // バッファのバインドを解除
    gl.bind_buffer(buffer_type, None);

    Ok(buffer)
}

// webXRの使用可否を確認して、webXRセッションを返す関数
#[wasm_bindgen]
pub async fn webxr_available(xrsystem: &XrSystem,document: &Document)->Result<Option<XrSession>,JsValue>{
    console::log_1(&"Starting WebXR Support Check".into());
    if let Some(is_supported) = JsFuture::from(
        xrsystem.is_session_supported(
            XrSessionMode::ImmersiveVr
        )
    ).await.unwrap().as_bool(){
        if is_supported{
            console::log_1(&"WebXR ImmersiveVr is Available!".into());
            let session_jsval = JsFuture::from(xrsystem.request_session(XrSessionMode::ImmersiveVr)).await?;
            if XrSession::instanceof(&session_jsval){
                let xrsession = XrSession::unchecked_from_js(session_jsval);
                Ok(Some(xrsession))
            }
            else{
                console::log_1(&"[Error] WebXR ImmersiveVr is Available but Session is not instance of Xrsession".into());
                display_error_page(document, "ImmersiveVr is Available but Session is not instance of Xrsession").await?;
                Err(JsValue::null())
            }
        }
        else{
            console::log_1(&"WebXR ImmersiveVr is not Available.".into());
            display_error_page(document, "ImmersiveVr is not Available.").await?;
            Ok(None)
        }
    }
    else{
        console::log_1(&"[Error] WebXR Support unknown.".into());
        display_error_page(document, "WebXR Support unknown.").await?;
        Err(JsValue::null())
    }
}

// webGL2の使用可否を確認して、コンテキストを返す関数
#[wasm_bindgen]
pub async fn create_webgl2_context(document: &Document)->Result<WebGl2RenderingContext,JsValue>{
    console::log_1(&"Try to get canvas".into());
    let Some(canvas) = document.query_selector("canvas")? else{
        console::log_1(&"[Error] canvas was none. Please check html file.".into());
        display_error_page(document,"canvas was none.").await?;
        return Err(JsValue::null());
    };
    let canvas = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;
    console::log_1(&"Got canvas".into());

    canvas.set_width(1920);
    canvas.set_height(1080);

    let Some(gl) = canvas.get_context("webgl2")? else{
        console::log_1(&"[Error] Could not get webgl2 context.".into());
        display_error_page(document,"Could not get webgl2 context.").await?;
        return Err(JsValue::null());
    };

    let gl = gl.dyn_into::<WebGl2RenderingContext>()?;
    Ok(gl)
}

// webXR等が私用できなかったときにエラーページを表示する関数
#[wasm_bindgen]
pub async fn display_error_page(document: &Document, error_msg: &str) -> Result<(), JsValue>{
    let body = document.body().expect("document doesn't have body.");
    let default_val = document.create_element("h1")?;
    default_val.set_text_content(Some("[Error Page]"));
    default_val.set_text_content(Some(error_msg));
    let _ = body.append_child(&default_val)?;
    Ok(())
}
