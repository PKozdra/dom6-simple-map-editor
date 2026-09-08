use dom6_mapgen::{Control, Sink, Stage};
use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

use crate::wire;

pub const SCRIPT: &str = "./mapgen-worker_loader.js";

fn post(scope: &DedicatedWorkerGlobalScope, bytes: &[u8]) {
    let array = Uint8Array::from(bytes);
    let buffer = array.buffer();
    let _ = scope.post_message_with_transfer(&array, &Array::of1(&buffer));
}

struct PostSink {
    scope: DedicatedWorkerGlobalScope,
}

impl Sink for PostSink {
    fn wants_hash(&self) -> bool {
        false
    }

    fn stage(&mut self, stage: Stage, _call: u32, _hash: u64) -> Control {
        post(&self.scope, &wire::encode_progress(stage, 0, 0));
        Control::Continue
    }

    fn progress(&mut self, stage: Stage, done: u32, total: u32) -> Control {
        post(&self.scope, &wire::encode_progress(stage, done, total));
        Control::Continue
    }
}

pub fn start() {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let inner = scope.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        let data = Uint8Array::new(&e.data()).to_vec();
        let job = match wire::decode_job(&data) {
            Ok(j) => j,
            Err(_) => {
                post(&inner, &[wire::CANCELLED]);
                return;
            }
        };
        let mut sink = PostSink {
            scope: inner.clone(),
        };
        match wire::run_job(&job, &mut sink) {
            Ok(g) => {
                let (planes, gates) = wire::planes_of(g);
                post(&inner, &wire::encode_done(&planes, &gates));
            }
            Err(_) => post(&inner, &[wire::CANCELLED]),
        }
    });
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    post(&scope, &[wire::READY]);
}
