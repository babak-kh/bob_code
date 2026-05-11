use tokio::process::Command;
use tokio::sync::mpsc;

pub struct NvidiaSmi;

impl NvidiaSmi {
    pub fn new() -> Self {
        NvidiaSmi
    }

    pub async fn get_gpu_info(&self) -> String {
        let output = Command::new("nvidia-smi")
            .args(&[
                "--query-gpu=name,utilization.gpu,memory.used",
                "--format=csv",
            ])
            .output()
            .await
            .expect("Failed to execute nvidia-smi command");
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    pub async fn monitor_gpu(self, sender: mpsc::UnboundedSender<String>) {
        loop {
            let gpu_info = self.get_gpu_info().await;
            if sender.send(gpu_info).is_err() {
                break; // Receiver dropped
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await; // Adjust the interval as needed
        }
    }
}
