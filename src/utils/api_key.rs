use crate::middleware::auth::hash_api_key;
use uuid::Uuid;

pub struct GeneratedApiKey {
    pub raw: String, // only shown once in the response
    pub hash: String,
    pub prefix: String,
}

pub fn generate_api_key() -> GeneratedApiKey {
    // format: sk_live_{uuid without hyphens}
    let raw = format!("sk_live_{}", Uuid::new_v4().to_string().replace("-", ""));
    let prefix = raw[..16].to_string();
    let hash = hash_api_key(&raw);

    GeneratedApiKey { raw, hash, prefix }
}
