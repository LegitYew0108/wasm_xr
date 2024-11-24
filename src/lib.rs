use wasm_bindgen::prelude::*;
use futures::stream::StreamExt;
use web_sys::{console,Response,Window,Document, XrFrame,XrSessionMode,HtmlCanvasElement,XrRenderStateInit,XrView, XrReferenceSpace,XrWebGlLayer, XrRigidTransform, XrSession,WebGl2RenderingContext};
use wasm_bindgen_futures::JsFuture;
use js_sys::Function;
use futures::channel::{oneshot, mpsc};
use gl_matrix::common::*;
use gl_matrix::{vec3,mat4,quat};
use std::rc::Rc;
use std::cell::RefCell;

pub struct GlProgram{
    gl: WebGl2RenderingContext,
    program: web_sys::WebGlProgram,
}

#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue>{

    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");


    console::log_1(&"Starting WebXR support check".into());
    let xrsystem = window.navigator().xr();

    console::log_2(&"XR system: ".into(), &xrsystem);
    if let Some(is_supported) = JsFuture::from(
        xrsystem.is_session_supported(
            XrSessionMode::ImmersiveVr)
    ).await?.as_bool(){
        if is_supported{
            console::log_1(&"WebXR is supported".into());
            let session_jsval = JsFuture::from(xrsystem.request_session(XrSessionMode::ImmersiveVr)).await?;
            if XrSession::instanceof(&session_jsval){
                console::log_1(&"Session create succeed".into());
                let session = XrSession::unchecked_from_js(session_jsval);
                create_webgl2_context(window,&document,session).await;
                console::log_1(&"webgl2 document created".into());
            }
            else{
                console::log_1(&"WebXR session could not created".into());
                let body = document.body().expect("document should have a body");
                let default_val = document.create_element("h1")?;
                default_val.set_text_content(Some("[Error Page] Could not create WebXR session"));
                body.append_child(&default_val)?;
                return Ok(());
            }
        }
        else{
            console::log_1(&"WebXR is not supported".into());
            let body = document.body().expect("document should have a body");
            let default_val = document.create_element("h1")?;
            default_val.set_text_content(Some("Sorry, WebXR is not supported on this device"));
            body.append_child(&default_val)?;
            return Ok(());
        }
    }
    else{
        console::log_1(&"[Error] WebXR support is unknown".into());
        let body = document.body().expect("document should have a body");
        let default_val = document.create_element("h1")?;
        default_val.set_text_content(Some("[Error Page] Could not determine WebXR support"));
        body.append_child(&default_val)?;
        return Ok(());
    }
    Ok(())
}

#[wasm_bindgen]
pub fn render_frame(time: f64, frame: &XrFrame, reference_space: &XrReferenceSpace, session: &XrSession, gl: &WebGl2RenderingContext, program: &web_sys::WebGlProgram){
    let pose = frame.get_viewer_pose(&reference_space);
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

    let mut projection = mat4::create();
    let projection_from_view = view.projection_matrix();
    projection = projection_from_view.try_into().unwrap();

    let model_location = gl.get_uniform_location(program, "model");
    let view_location = gl.get_uniform_location(program, "view");
    let projection_location = gl.get_uniform_location(program, "projection");
    gl.uniform_matrix4fv_with_f32_array(model_location.as_ref(), false, &model);
    gl.uniform_matrix4fv_with_f32_array(view_location.as_ref(), false, &view_matrix);
    gl.uniform_matrix4fv_with_f32_array(projection_location.as_ref(), false, &projection);
}

#[derive(Debug, Clone)]
enum Shader{
    Vertex(String),
    Fragment(String),
}


#[wasm_bindgen]
pub async fn create_webgl2_context(window: Window,document: &Document,session: XrSession){
    let canvas = document.query_selector("canvas").unwrap().unwrap();
    // HtmlCanvasElementを取得
    let Ok(canvas_element) = canvas.dyn_into::<web_sys::HtmlCanvasElement>() else{
        console::log_1(&"Error: could not convert to HtmlCanvasElement".into());
        panic!();
    };

    canvas_element.set_width(1920);
    canvas_element.set_height(1080);
    
    console::log_1(&"set canvas success".into());

    let Ok(gl) = canvas_element.get_context("webgl2") else{
        console::log_1(&"Error: could not get webgl2 context".into());
        panic!();
    };

    let Some(gl) = gl else{
        console::log_1(&"Error: could not get webgl2 context".into());
        panic!();
    };

    let Ok(gl) = gl.dyn_into::<web_sys::WebGl2RenderingContext>() else{
        console::log_1(&"Error: could not convert to WebGl2RenderingContext".into());
        panic!();
    };

    let(mut tx, mut rx) = mpsc::channel::<Shader>(32);

    let mut vertex_tx = tx.clone();

    wasm_bindgen_futures::spawn_local(async move{
        console::log_1(&"vertex shader read start".into());

        // vertex shaderを読み出す
        let Ok(vertex_shader_source) = wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str("../shader/vertex_shader.glsl"),
        )
        .await else{
            console::log_1(&"shader read failed".into());
            panic!("shader read failed");
        };
        let Ok(vertex_shader_source) = vertex_shader_source
        .dyn_into::<Response>() else{
            console::log_1(&"dynamic cast to Response failed".into());
            panic!("shader read failed");
        };
        let Ok(vertex_shader_source) = vertex_shader_source.text() else{
            console::log_1(&"could not change to text".into());
            panic!("shader read failed");
        };
        let Ok(vertex_shader_source) = wasm_bindgen_futures::JsFuture::from(vertex_shader_source).await else{
            console::log_1(&"promise failed".into());
            panic!("shader read failed");
        };

        let Some(vertex_shader_source) = vertex_shader_source.as_string() else{
            console::log_1(&"shader source none".into());
            panic!("shader read failed");
        };
        
        let value = Shader::Vertex(vertex_shader_source);
        loop{
            let Err(_) = vertex_tx.try_send(value.clone()) else{
                break;
            };
        }

        console::log_1(&"fragment shader read start".into());

        // fragment shaderを読み出す
        let Ok(fragment_shader_source) = wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str("../shader/fragment_shader.glsl"),
        )
        .await else{
            console::log_1(&"shader read failed".into());
            panic!("shader read failed");
        };
        let Ok(fragment_shader_source) = fragment_shader_source
        .dyn_into::<Response>() else{
            console::log_1(&"dynamic cast to Response failed".into());
            panic!("shader read failed");
        };
        let Ok(fragment_shader_source) = fragment_shader_source.text() else{
            console::log_1(&"could not change to text".into());
            panic!("shader read failed");
        };
        let Ok(fragment_shader_source) = wasm_bindgen_futures::JsFuture::from(fragment_shader_source).await else{
            console::log_1(&"promise failed".into());
            panic!("shader read failed");
        };

        let Some(fragment_shader_source) = fragment_shader_source.as_string() else{
            console::log_1(&"shader source none".into());
            panic!("shader read failed");
        };
        
        let value = Shader::Fragment(fragment_shader_source);
        loop{
            let Err(_) = tx.try_send(value.clone()) else{
                break;
            };
        }
    });

    wasm_bindgen_futures::spawn_local(async move{
        let mut vertex_shader_source:Option<String> = None;
        let mut is_vertex_received = false;
        let mut fragment_shader_source:Option<String> = None;
        let mut is_fragment_received = false;
        while let message = rx.next().await {
            let Some(message) = message else{
                console::log_1(&"message none".into());
                break;
            };
            match message{
                Shader::Vertex(source) => {
                    console::log_1(&"vertex shader".into());
                    console::log_1(&source.clone().into());
                    vertex_shader_source = Some(source);
                    is_vertex_received = true;
                },
                Shader::Fragment(source) => {
                    console::log_1(&"fragment shader".into());
                    console::log_1(&source.clone().into());
                    fragment_shader_source = Some(source);
                    is_fragment_received = true;
                },
            }

            if is_vertex_received && is_fragment_received{
                break;
            }
        };
        // シェーダー受取完了

        // シェーダーのコンパイル
        let vertex_shader = gl.create_shader(WebGl2RenderingContext::VERTEX_SHADER).unwrap();
        let fragment_shader = gl.create_shader(WebGl2RenderingContext::FRAGMENT_SHADER).unwrap();

        gl.shader_source(&vertex_shader, &vertex_shader_source.unwrap());
        gl.compile_shader(&vertex_shader);
        let vertex_status = gl
            .get_shader_parameter(&vertex_shader, WebGl2RenderingContext::COMPILE_STATUS)
            .as_bool()
            .unwrap();
        if !vertex_status {
            let log = gl.get_shader_info_log(&vertex_shader).unwrap();
            console::log_1(&log.into());
        }

        gl.shader_source(&fragment_shader, &fragment_shader_source.unwrap());
        gl.compile_shader(&fragment_shader);
        let fragment_status = gl
            .get_shader_parameter(&fragment_shader, WebGl2RenderingContext::COMPILE_STATUS)
            .as_bool()
            .unwrap();
        if !fragment_status {
            let log = gl.get_shader_info_log(&fragment_shader).unwrap();
            console::log_1(&log.into());
        }
        console::log_1(&"shader compile success".into());

        let Some(program) = gl.create_program() else{
            console::log_1(&"program none value".into());
            panic!("program none value");
        };

        gl.attach_shader(&program, &vertex_shader);
        gl.attach_shader(&program, &fragment_shader);
        gl.link_program(&program);

        let link_status = gl.get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
            .as_bool()
            .unwrap();
        if !link_status{
            let log = gl.get_program_info_log(&program).unwrap();
            console::log_1(&log.into());
        }

        // プログラムを使用
        gl.use_program(Some(&program));
        console::log_1(&"use program success".into());

        const VERTEX_SIZE: i32 = 3;
        const COLOR_SIZE: i32 = 4;

        const FLOAT32_BYTES_PER_ELEMENT: i32 = 4;
        const STRIDE: i32 = (VERTEX_SIZE + COLOR_SIZE) * FLOAT32_BYTES_PER_ELEMENT;
        const POSITION_OFFSET: i32 = 0;
        const COLOR_OFFSET: i32 = VERTEX_SIZE * FLOAT32_BYTES_PER_ELEMENT;
        let vertices: [f32; 56] = [
            0.0, 30.0, 0.0,  // 座標
            1.0, 1.0, 1.0, 1.0,      // 色
            0.0, 30.0, 30.0,  // 座標
            0.0, 1.0, 1.0, 1.0,      // 色
            30.0, 30.0, 30.0,  // 座標
            1.0, 0.0, 1.0, 1.0,      // 色
            30.0, 30.0, 0.0,  // 座標
            0.0, 1.0, 0.0, 1.0,      // 色
            0.0, 0.0, 0.0,  // 座標
            1.0, 1.0, 0.0, 1.0,      // 色
            0.0, 0.0, 30.0,  // 座標
            1.0, 0.0, 1.0, 1.0,      // 色
            30.0, 0.0, 30.0,  // 座標
            1.0, 0.0, 0.0, 1.0,      // 色
            30.0, 0.0, 0.0,  // 座標
            0.0, 1.0, 1.0, 1.0,      // 色
        ];
        let indices: [u16; 36] =[
            0, 2, 1,
            0, 3, 2,
            0, 4, 1,
            1, 5, 4,
            1, 2, 5,
            2, 6, 5,
            2, 3, 6,
            3, 7, 6,
            3, 0, 7,
            0, 4, 7,
            4, 5, 7,
            5, 6, 7,
        ];

        let interleaved_buffer = create_f32_buffer(WebGl2RenderingContext::ARRAY_BUFFER, &vertices, &gl).await.unwrap();
        let index_buffer = create_u16_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, &indices, &gl).await.unwrap();

        gl.enable(WebGl2RenderingContext::DEPTH_TEST);
        gl.enable(WebGl2RenderingContext::CULL_FACE);

        let vertex_attrib_location = gl.get_attrib_location(&program, "vertex_position");
        let color_attrib_location = gl.get_attrib_location(&program, "color");

        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&interleaved_buffer));
        gl.enable_vertex_attrib_array(vertex_attrib_location as u32);
        gl.enable_vertex_attrib_array(color_attrib_location as u32);
        gl.vertex_attrib_pointer_with_i32(vertex_attrib_location as u32, VERTEX_SIZE, WebGl2RenderingContext::FLOAT, false, STRIDE, POSITION_OFFSET);
        gl.vertex_attrib_pointer_with_i32(color_attrib_location as u32, COLOR_SIZE, WebGl2RenderingContext::FLOAT, false, STRIDE, COLOR_OFFSET);

        let index_size = indices.len() as i32;
        gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&index_buffer));
        let gl_program = GlProgram{gl,program};


        session.add_event_listener_with_callback("end", &js_sys::Function::new_no_args("on_session_end")).unwrap();
        let gl = gl_program.gl;
        let program = gl_program.program;
        let render_state = XrRenderStateInit::new();
        let webgl_layer = XrWebGlLayer::new_with_web_gl2_rendering_context(&session, &gl).unwrap();
        render_state.set_base_layer(Some(&webgl_layer));
        session.update_render_state_with_state(&render_state);
        let reference_space = JsFuture::from(session.request_reference_space(web_sys::XrReferenceSpaceType::Local)).await.unwrap();
        if XrReferenceSpace::instanceof(&reference_space){
            let reference_space = XrReferenceSpace::unchecked_from_js(reference_space);
            let offset_space = XrRigidTransform::new().unwrap();
            reference_space.get_offset_reference_space(&offset_space);

            let animation_loop = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64, XrFrame)>>));
            //初期状態はNone
            //RefCellはClosureを後で自分自身を参照できるようにするためのラッパー
            //Rcは参照カウントを増やすためのラッパー
            
            //クローンを作成
            let animation_loop_clone = Rc::clone(&animation_loop);
            let session_clone = session.clone();
            *animation_loop.borrow_mut() = Some(Closure::wrap(Box::new(move |time: f64, frame: XrFrame|{
                render_frame(time, &frame, &reference_space, &session_clone, &gl, &program);
                session_clone.request_animation_frame(animation_loop_clone.borrow().as_ref().unwrap().as_ref().unchecked_ref::<js_sys::Function>());
            }) as Box<dyn FnMut(f64, XrFrame)>));

            //最初のアニメーションフレームをリクエスト
            let animation_frame_request_id = session.request_animation_frame(&animation_loop.borrow().as_ref().unwrap().as_ref().unchecked_ref::<js_sys::Function>());
    }
    });
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
