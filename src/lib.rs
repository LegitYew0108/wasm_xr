use wasm_bindgen::prelude::*;
use web_sys::{console,XrSessionMode, XrReferenceSpace, XrRigidTransform, XrSession, XrRenderStateInit};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(start)]
pub async fn run() -> Result<(), JsValue>{

    console::log_1(&"Starting WebXR support check".into());
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
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
                let body = document.body().expect("document should have a body");
                let default_val = document.create_element("h1")?;
                default_val.set_text_content(Some("Session create succeed!"));
                body.append_child(&default_val)?;
                let session = XrSession::unchecked_from_js(session_jsval);
                run_session(session).await?
            }
            else{
                console::log_1(&"WebXR session could not created".into());
                let body = document.body().expect("document should have a body");
                let default_val = document.create_element("h1")?;
                default_val.set_text_content(Some("[Error Page] Could not create WebXR session"));
                body.append_child(&default_val)?;
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
pub async fn run_session(session: XrSession) -> Result<(), JsValue>{
    session.add_event_listener_with_callback("end", &js_sys::Function::new_no_args("on_session_end"))?;
    let reference_space = JsFuture::from(session.request_reference_space(web_sys::XrReferenceSpaceType::LocalFloor)).await?;
    if XrReferenceSpace::instanceof(&reference_space){
        let reference_space = XrReferenceSpace::unchecked_from_js(reference_space);
        let offset_space = XrRigidTransform::new()?;
        reference_space.get_offset_reference_space(&offset_space);
        let animation_frame_request_id = session.request_animation_frame(&js_sys::Function::new_no_args("on_animation_frame"));
    }
    Ok(())
}

#[wasm_bindgen]
pub fn on_animation_frame(){
    console::log_1(&"Animation frame".into());
}
