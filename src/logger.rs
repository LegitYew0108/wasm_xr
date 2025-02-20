use web_sys::*;
use wasm_bindgen::prelude::*;

pub struct Logger{
    last_time: f64,
    first_time: f64,
    frame_count: u32,
    total_frames: u32,
    performance: Performance,
}

impl Logger{
    pub fn new(performance:Performance)->Self{
        Logger{
            last_time: performance.now(),
            first_time: performance.now(),
            frame_count: 0,
            total_frames: 0,
            performance,
        }
    }

    pub fn track_frame(&mut self){
        self.frame_count += 1;
        let now = self.performance.now();
        let elapsed = now - self.last_time;
        self.total_frames += 1;
        if elapsed > 1000.0{
            self.log_fps();
            self.last_time = now;
            self.frame_count = 0; }
    }

    pub fn log_fps(&self){
        let fps = self.frame_count as f64 / (self.performance.now() - self.last_time) * 1000.0;
        console::log_1(&format!("FPS: {}", fps).into());
        let avg_fps = self.total_frames as f64 / (self.performance.now() - self.first_time) * 1000.0;
        console::log_1(&format!("Average FPS: {}", avg_fps).into());
    }

    pub fn log_memory_usage(&self) {
        // メモリ情報の取得
        if let Ok(memory) = js_sys::Reflect::get(&self.performance, &"memory".into()) {
            if let Some(memory) = memory.dyn_ref::<js_sys::Object>() {
                let js_heap_size_limit = js_sys::Reflect::get(memory, &"jsHeapSizeLimit".into())
                    .unwrap_or_else(|_| 0.into())
                    .as_f64()
                    .unwrap_or(0.0);

                let total_js_heap_size = js_sys::Reflect::get(memory, &"totalJSHeapSize".into())
                    .unwrap_or_else(|_| 0.into())
                    .as_f64()
                    .unwrap_or(0.0);

                let used_js_heap_size = js_sys::Reflect::get(memory, &"usedJSHeapSize".into())
                    .unwrap_or_else(|_| 0.into())
                    .as_f64()
                    .unwrap_or(0.0);

                console::log_1(&format!(
                    "Memory Usage:\nHeap Size Limit: {:.2} MB\nTotal JS Heap Size: {:.2} MB\nUsed JS Heap Size: {:.2} MB",
                    js_heap_size_limit / 1_048_576.0,
                    total_js_heap_size / 1_048_576.0,
                    used_js_heap_size / 1_048_576.0
                ).into());
            }
        }
    }
}
