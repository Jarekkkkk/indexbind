use crate::IndexbindError;
use ndarray::Array2;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;

#[derive(Clone)]
pub struct CrossEncoder {
    inner: Arc<Mutex<Option<CrossEncoderInner>>>,
}

struct CrossEncoderInner {
    session: Session,
    tokenizer: Tokenizer,
    max_length: usize,
}

impl CrossEncoder {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub fn ensure_loaded(&self) -> Result<(), IndexbindError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| IndexbindError::Internal(e.to_string()))?;
        if guard.is_some() {
            return Ok(());
        }
        let api = hf_hub::api::sync::Api::new()
            .map_err(|e| IndexbindError::Internal(e.to_string()))?;
        let model_id = "onnx-community/bge-reranker-v2-m3-ONNX".to_string();
        let api_model = api.model(model_id);
        let model_path = api_model
            .get("onnx/model_quantized.onnx")
            .map_err(|e| IndexbindError::Internal(format!("failed to download model: {e}")))?;
        let tokenizer_path = api_model
            .get("tokenizer.json")
            .map_err(|e| IndexbindError::Internal(format!("failed to download tokenizer: {e}")))?;

        let session = Session::builder()
            .map_err(|e| IndexbindError::Internal(format!("ort builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| IndexbindError::Internal(format!("ort opt level: {e}")))?
            .with_intra_threads(4)
            .map_err(|e| IndexbindError::Internal(format!("ort threads: {e}")))?
            .commit_from_file(&model_path)
            .map_err(|e| IndexbindError::Internal(format!("ort commit: {e}")))?;
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| IndexbindError::Internal(format!("tokenizer load: {e}")))?;

        *guard = Some(CrossEncoderInner {
            session,
            tokenizer,
            max_length: 512,
        });
        Ok(())
    }

    pub fn rerank(
        &self,
        query: &str,
        passages: &[String],
        batch_size: usize,
    ) -> Result<Vec<f32>, IndexbindError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| IndexbindError::Internal(e.to_string()))?;
        let inner = guard
            .as_mut()
            .ok_or_else(|| IndexbindError::Internal("cross-encoder not loaded".into()))?;

        let mut all_scores = Vec::with_capacity(passages.len());
        for chunk in passages.chunks(batch_size) {
            let pairs: Vec<(&str, &str)> = chunk.iter().map(|p| (query, p.as_str())).collect();
            let encodings = inner
                .tokenizer
                .encode_batch(pairs, true)
                .map_err(|e| IndexbindError::Internal(e.to_string()))?;

            let batch_actual = encodings.len();
            let batch_max = encodings
                .iter()
                .map(|e| e.len().min(inner.max_length))
                .max()
                .unwrap_or(1);

            let mut input_ids = Array2::zeros((batch_actual, batch_max));
            let mut attention_mask = Array2::zeros((batch_actual, batch_max));

            for (i, encoding) in encodings.iter().enumerate() {
                let ids = encoding.get_ids();
                let len = ids.len().min(batch_max);
                for j in 0..len {
                    input_ids[[i, j]] = ids[j] as i64;
                    attention_mask[[i, j]] = 1i64;
                }
            }

            let outputs = inner
                .session
                .run(ort::inputs![
                    Tensor::from_array(input_ids)
                        .map_err(|e| IndexbindError::Internal(format!("ort tensor input_ids: {e}")))?,
                    Tensor::from_array(attention_mask)
                        .map_err(|e| IndexbindError::Internal(format!("ort tensor attention_mask: {e}")))?,
                ])
                .map_err(|e| IndexbindError::Internal(format!("ort run: {e}")))?;

            let (_shape, logits_data) = outputs["logits"]
                .try_extract_tensor::<f32>()
                .map_err(|e| IndexbindError::Internal(format!("ort extract: {e}")))?;

            for i in 0..batch_actual {
                let pos = logits_data[i * 2 + 1];
                let neg = logits_data[i * 2];
                let prob = 1.0 / (1.0 + (-pos + neg).exp());
                all_scores.push(prob);
            }
        }
        Ok(all_scores)
    }
}

impl Default for CrossEncoder {
    fn default() -> Self {
        Self::new()
    }
}
