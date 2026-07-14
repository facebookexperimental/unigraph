// Copyright (c) Meta Platforms, Inc. and affiliates.

use ll::reporters::DONTPRINT_TAG;
use ll::reporters::EventQueue;
use ll::reporters::Reporter;
use ll::reporters::TaskEvent;
use ll::task_tree::TaskInternal;
use ll::task_tree::TaskResult;
use ll::task_tree::TaskStatus;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;

pub struct ConsoleReporter {
    pub log_task_start: bool,
}

impl Default for ConsoleReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleReporter {
    pub fn new() -> Self {
        Self {
            log_task_start: false,
        }
    }
}

impl Reporter for ConsoleReporter {
    fn start(&self, queue: EventQueue) {
        let log_task_start = self.log_task_start;

        // Set up a JS setInterval to drain the queue every 50ms.
        // Works in both browser (window.setInterval) and Node.js.
        let tick = Closure::wrap(Box::new(move || {
            let events = std::mem::take(&mut *queue.lock().unwrap());
            for event in events {
                match event {
                    TaskEvent::Start(task) => {
                        if log_task_start && !task.tags.contains(DONTPRINT_TAG) {
                            let msg = format!("[START] {}", task.full_name());
                            web_sys::console::log_1(&JsValue::from_str(&msg));
                        }
                    }
                    TaskEvent::End(task) => {
                        if task.tags.contains(DONTPRINT_TAG) {
                            continue;
                        }
                        let name = task.full_name();
                        let duration = format_duration(&task);
                        let data = format_data(&task);
                        match &task.status {
                            TaskStatus::Finished(TaskResult::Failure(err), _) => {
                                let msg = format!("[ERR]{duration} {name}{data}\n{err}");
                                web_sys::console::error_1(&JsValue::from_str(&msg));
                            }
                            _ => {
                                let msg = format!("[OK]{duration} {name}{data}");
                                web_sys::console::log_1(&JsValue::from_str(&msg));
                            }
                        }
                    }
                    TaskEvent::Progress(_) => {}
                }
            }
        }) as Box<dyn FnMut()>);

        // Use js_sys global setInterval — works in both browser and Node.js
        let set_interval = js_sys::Reflect::get(&js_sys::global(), &"setInterval".into())
            .expect("setInterval not found in global scope");
        let set_interval: js_sys::Function = set_interval.into();
        set_interval
            .call2(&JsValue::NULL, tick.as_ref(), &JsValue::from_f64(50.0))
            .expect("setInterval call failed");

        tick.forget();
    }
}

fn format_duration(task: &TaskInternal) -> String {
    if let TaskStatus::Finished(_, finished_at) = &task.status {
        if let Ok(d) = finished_at.duration_since(task.started_at) {
            return format!(" [{} ms]", d.as_millis());
        }
    }
    String::new()
}

fn format_data(task: &TaskInternal) -> String {
    let entries: Vec<String> = task
        .all_data()
        .filter(|(_, entry)| !entry.1.contains(DONTPRINT_TAG))
        .map(|(k, entry)| format!("{k}={}", entry.0))
        .collect();

    if entries.is_empty() {
        String::new()
    } else {
        format!(" ({})", entries.join(", "))
    }
}
