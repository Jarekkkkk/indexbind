use crate::{error::Result, InkdexError};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbeddingBackend {
    Fastembed {
        model_name: String,
        model_code: String,
    },
    Hashing {
        dimensions: usize,
    },
}

impl Default for EmbeddingBackend {
    fn default() -> Self {
        Self::Hashing { dimensions: 256 }
    }
}

pub struct Embedder {
    backend: EmbeddingBackend,
}

impl Embedder {
    pub fn new(backend: EmbeddingBackend) -> Result<Self> {
        if matches!(backend, EmbeddingBackend::Fastembed { .. }) {
            return Err(InkdexError::Embedding(anyhow!(
                "fastembed backend is not enabled in this build"
            )));
        }
        Ok(Self { backend })
    }

    pub fn backend(&self) -> &EmbeddingBackend {
        &self.backend
    }

    pub fn embed_passages(&mut self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_prefixed("passage: ", inputs)
    }

    pub fn embed_queries(&mut self, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_prefixed("query: ", inputs)
    }

    fn embed_prefixed(&mut self, prefix: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        match &self.backend {
            EmbeddingBackend::Fastembed { .. } => Err(InkdexError::Embedding(anyhow!(
                "fastembed backend is not enabled in this build"
            ))),
            EmbeddingBackend::Hashing { dimensions } => Ok(inputs
                .iter()
                .map(|value| hashing_embedding(&format!("{prefix}{value}"), *dimensions))
                .collect()),
        }
    }
}

pub fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>()
}

pub fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let (mut dot, mut left_norm, mut right_norm) = (0.0f32, 0.0f32, 0.0f32);
    for (l, r) in left.iter().zip(right.iter()) {
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn hashing_embedding(input: &str, dimensions: usize) -> Vec<f32> {
    let mut vector = vec![0.0f32; dimensions];
    for token in input.split_whitespace() {
        let hash = blake3::hash(token.as_bytes());
        let bytes = hash.as_bytes();
        let bucket = usize::from(bytes[0]) % dimensions;
        let sign = if bytes[1] % 2 == 0 { 1.0 } else { -1.0 };
        vector[bucket] += sign;
    }
    normalize(&mut vector);
    vector
}

fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm == 0.0 {
        return;
    }
    for value in vector {
        *value /= norm;
    }
}
