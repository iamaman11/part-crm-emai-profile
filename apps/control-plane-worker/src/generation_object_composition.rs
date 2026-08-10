use cloudflare_adapters::r2_generation_objects::R2GenerationObjects;
use worker::{Env, Result};

pub fn generation_objects(env: &Env) -> Result<R2GenerationObjects> {
    Ok(R2GenerationObjects::new(env.bucket("PROFILE_GENERATIONS")?))
}

#[cfg(test)]
mod tests {
    const SOURCE: &str = include_str!("generation_object_composition.rs");

    #[test]
    fn composition_uses_only_the_profile_generation_r2_binding() {
        assert!(SOURCE.contains("env.bucket(\"PROFILE_GENERATIONS\")"));
        assert!(!SOURCE.contains("D1Database"));
        assert!(!SOURCE.contains("KvStore"));
    }
}
