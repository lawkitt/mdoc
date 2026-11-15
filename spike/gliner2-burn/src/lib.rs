//! GLiNER2-PII's eight ONNX fragments imported by burn-onnx 0.22, run as one chain.
//! Spike only (lawkitt/mdoc#903, decision 29).
#![allow(clippy::all)]

macro_rules! fragment {
    ($m:ident, $f:literal) => {
        #[allow(unused, clippy::all)]
        pub mod $m {
            include!(concat!(env!("OUT_DIR"), "/model/", $f, ".rs"));
        }
    };
}
fragment!(encoder, "encoder");
fragment!(token_gather, "token_gather");
fragment!(span_rep, "span_rep");
fragment!(schema_gather, "schema_gather");
fragment!(count_pred, "count_pred_argmax");
fragment!(count_lstm, "count_lstm_fixed");
fragment!(scorer, "scorer");
fragment!(classifier, "classifier");

use burn::prelude::*;
use burn::tensor::TensorData;

pub const PRECISION: &str = env!("SPIKE_PREC");

fn bpk(name: &str) -> String {
    format!("{}/model/{name}_{PRECISION}.bpk", env!("OUT_DIR"))
}

/// `GLINER2_BURN=flex|cpu|metal`, or `None` for the ORT path.
pub fn requested() -> Option<String> {
    std::env::var("GLINER2_BURN").ok().filter(|v| !v.is_empty() && v != "0")
}

pub fn device(name: &str) -> Device {
    match name {
        #[cfg(feature = "flex")]
        "flex" => Device::flex(),
        #[cfg(feature = "cpu")]
        "cpu" => Device::cpu(),
        #[cfg(feature = "metal")]
        "metal" => Device::metal(Default::default()),
        other => panic!("GLINER2_BURN={other}: backend not compiled in"),
    }
}

pub struct Fragments {
    pub device: Device,
    encoder: encoder::Model,
    token_gather: token_gather::Model,
    span_rep: span_rep::Model,
    schema_gather: schema_gather::Model,
    count_pred: count_pred::Model,
    count_lstm: count_lstm::Model,
    scorer: scorer::Model,
}

/// One entity schema group: `[prompt token, field tokens…]`.
pub struct TaskIn {
    pub schema_indices: Vec<i64>,
}

/// `pred_count` and, when it is non-zero, the scorer's `[MAX_COUNT, W, mw, M]` scores.
pub struct TaskOut {
    pub pred_count: i64,
    pub scores: Option<Vec<f32>>,
}

fn ints<const D: usize>(data: Vec<i64>, shape: [usize; D], device: &Device) -> Tensor<D, Int> {
    Tensor::<D, Int>::from_data(TensorData::new(data, shape), device)
}

impl Fragments {
    pub fn load(name: &str) -> Self {
        let device = device(name);
        Self {
            encoder: encoder::Model::from_file(bpk("encoder"), &device),
            token_gather: token_gather::Model::from_file(bpk("token_gather"), &device),
            span_rep: span_rep::Model::from_file(bpk("span_rep"), &device),
            schema_gather: schema_gather::Model::from_file(bpk("schema_gather"), &device),
            count_pred: count_pred::Model::from_file(bpk("count_pred_argmax"), &device),
            count_lstm: count_lstm::Model::from_file(bpk("count_lstm_fixed"), &device),
            scorer: scorer::Model::from_file(bpk("scorer"), &device),
            device,
        }
    }

    /// The span chain for entity groups, intermediates left on the device.
    #[allow(clippy::too_many_arguments)]
    pub fn run_entities(
        &self,
        input_ids: Vec<i64>,
        attention_mask: Vec<i64>,
        word_positions: Vec<i64>,
        span_idx: Vec<i64>,
        num_words: usize,
        max_width: usize,
        tasks: &[TaskIn],
    ) -> Vec<TaskOut> {
        let d = &self.device;
        let seq = input_ids.len();
        let hidden = self.encoder.forward(ints(input_ids, [1, seq], d), ints(attention_mask, [1, seq], d));
        let text_embs = self.token_gather.forward(hidden.clone(), ints(word_positions, [num_words], d));
        let span_emb = self
            .span_rep
            .forward(text_embs, ints(span_idx, [1, num_words * max_width, 2], d));
        tasks
            .iter()
            .map(|task| {
                let n = task.schema_indices.len();
                let (pc_emb, field_embs) =
                    self.schema_gather.forward(hidden.clone(), ints(task.schema_indices.clone(), [n], d));
                let pred_count = self.count_pred.forward(pc_emb).into_data().convert::<i64>().to_vec::<i64>().unwrap()[0];
                if pred_count <= 0 {
                    return TaskOut { pred_count, scores: None };
                }
                let struct_proj = self.count_lstm.forward(field_embs);
                let scores = self.scorer.forward(span_emb.clone(), struct_proj);
                let scores = scores.into_data().convert::<f32>().to_vec::<f32>().unwrap();
                TaskOut { pred_count, scores: Some(scores) }
            })
            .collect()
    }
}
