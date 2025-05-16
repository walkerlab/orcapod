use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct StreamInfo {
    // Fields for StreamInfo
}

#[derive(Debug)]
pub struct PipelineRun {
    node_outputs: RwLock<HashMap<String, StreamInfo>>,
    // Other fields for PipelineRun
}

impl PipelineRun {
    pub async fn process_node(
        self: Arc<Self>,
        node_key: String,
        input_map: HashMap<String, StreamInfo>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // ...existing code...

        // When spawning, clone Arc for each child
        for node in children_node_to_start {
            let input_map = self
                .node_outputs
                .read()
                .await
                .get(&node_key)
                .unwrap()
                .clone();

            let self_clone = Arc::clone(&self);
            tokio::spawn(async move {
                let _ = self_clone.process_node(node, input_map).await;
            });
        }

        Ok(())
    }
}

// Result type alias for convenience
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;