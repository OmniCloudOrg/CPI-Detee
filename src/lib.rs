// File: cpi_detee/src/lib.rs
use lib_cpi::{
    ActionParameter, ActionDefinition, ActionResult, CpiExtension, ParamType,
    action, param, validation
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;
use std::io::Write;
use std::fs::File;
use std::path::PathBuf;
use tempfile::tempdir;

#[no_mangle]
pub extern "C" fn get_extension() -> *mut dyn CpiExtension {
    Box::into_raw(Box::new(DeeTeeExtension::new()))
}

/// DeeTEE provider implemented as a dynamic extension
pub struct DeeTeeExtension {
    name: String,
    provider_type: String,
    default_settings: HashMap<String, Value>,
}

// Struct definitions for mapping DeeTEE CLI outputs

#[derive(Deserialize, Serialize, Debug)]
struct TestInstallResult {
    version: String,
    #[serde(default = "bool_true")]
    success: bool,
}

#[derive(Deserialize, Serialize, Debug)]
struct SetupContainerResult {
    container_id: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct AccountInfo {
    config_path: String,
    brain_url: String,
    ssh_key_path: String,
    wallet_public_key: String,
    account_balance: String,
    wallet_secret_key_path: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct CreateWorkerResult {
    hostname: Option<String>,
    price: String,
    total_units: i64,
    locked_lp: f64,
    ssh_port: i64,
    ssh_host: String,
    uuid: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
struct WorkerInfo {
    city: String,
    uuid: String,
    hostname: String,
    cores: i64,
    memory_mb: i64,
    disk_gb: i64,
    lp_per_hour: f64,
    time_left: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct UpdateWorkerResult {
    hardware_modified: Option<bool>,
    hours_updated: Option<i64>,
    #[serde(default = "bool_true")]
    success: bool,
}

// Helper function for default true value
fn bool_true() -> bool {
    true
}

impl DeeTeeExtension {
    pub fn new() -> Self {
        let mut default_settings = HashMap::new();
        default_settings.insert("distro".to_string(), json!("ubuntu"));
        default_settings.insert("vcpus".to_string(), json!(2));
        default_settings.insert("memory_mb".to_string(), json!(2048));
        default_settings.insert("disk_gb".to_string(), json!(20));
        default_settings.insert("hours".to_string(), json!(4));

        Self {
            name: "detee".to_string(),
            provider_type: "command".to_string(),
            default_settings,
        }
    }
    
    // Helper method to run commands through docker exec on the DeeTEE CLI container
    fn run_detee_cmd(&self, command: &str) -> Result<String, String> {
        println!("Running DeeTEE command: {}", command);
        
        let parts: Vec<&str> = command.split_whitespace().collect();
        let mut cmd_args = vec!["exec", "-i", "detee-cli"];
        cmd_args.extend_from_slice(&parts);
        
        let output = Command::new("docker")
            .args(&cmd_args)
            .output()
            .map_err(|e| format!("Failed to execute DeeTEE command: {}", e))?;
            
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            println!("Command output: {}", stdout);
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format!("DeeTEE command failed: {}", stderr))
        }
    }
    
    // Run an arbitrary shell command
    fn run_shell_cmd(&self, command: &str) -> Result<String, String> {
        println!("Running shell command: {}", command);
        
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(|e| format!("Failed to execute shell command: {}", e))?;
            
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format!("Shell command failed: {}", stderr))
        }
    }
    
    // Parse table output from DeeTEE CLI into a vector of WorkerInfo
    fn parse_workers_table(&self, output: &str) -> Vec<WorkerInfo> {
        let mut workers = Vec::new();
        
        // Split the output by lines
        let lines: Vec<&str> = output.lines()
            .filter(|line| line.contains("|"))  // Only consider lines with pipe characters
            .collect();
        
        // Skip the header lines (first 2 lines) and separator line
        for line in lines.iter().skip(2) {
            // Skip separator lines
            if line.contains("----") {
                continue;
            }
            
            // Split the line by the pipe character and trim whitespace
            let columns: Vec<&str> = line.split('|')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            
            // Skip if we don't have enough columns
            if columns.len() < 7 {
                continue;
            }
            
            // Parse the worker information from columns
            let worker = WorkerInfo {
                city: columns[0].to_string(),
                uuid: columns[1].to_string(),
                hostname: columns[2].to_string(),
                cores: columns[3].parse().unwrap_or(0),
                memory_mb: columns[4].parse().unwrap_or(0),
                disk_gb: columns[5].parse().unwrap_or(0),
                lp_per_hour: columns[6].parse().unwrap_or(0.0),
                time_left: columns[7].to_string(),
            };
            
            workers.push(worker);
        }
        
        workers
    }
    
    // Parse command output based on the expected data
    fn parse_output<T: for<'de> Deserialize<'de>>(&self, output: &str) -> Result<T, String> {
        // This is a simplified implementation. In a real-world scenario, you would need 
        // to write more robust parsers for each command's output format.
        
        // Create a temporary directory to store the JSON
        let dir = tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
        let file_path = dir.path().join("output.json");
        
        // Create a JSON object from the command output
        let json_obj = self.cli_output_to_json(output, &file_path)?;
        
        // Deserialize the JSON into the target struct
        let result: T = serde_json::from_value(json_obj)
            .map_err(|e| format!("Failed to parse output: {}", e))?;
            
        Ok(result)
    }
    
    // Convert CLI text output to a JSON structure based on patterns
    fn cli_output_to_json(&self, output: &str, file_path: &PathBuf) -> Result<Value, String> {
        // This method would need to be customized for each command output format
        // The implementation below is a simplified example
        
        // Check for version information
        if output.contains("detee-cli") {
            let version = output.trim()
                .replace("detee-cli ", "")
                .trim()
                .to_string();
                
            return Ok(json!({
                "version": version,
                "success": true
            }));
        }
        
        // Check for container ID
        if output.len() == 64 || output.len() == 12 {
            // Likely a container ID (either full or short format)
            return Ok(json!({
                "container_id": output.trim()
            }));
        }
        
        // Check for account information
        if output.contains("Config path:") && output.contains("brain URL") {
            let mut account_info = json!({});
            
            // Extract config path
            if let Some(line) = output.lines().find(|l| l.contains("Config path:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let path = parts[1].trim();
                    account_info["config_path"] = json!(path);
                }
            }
            
            // Extract brain URL
            if let Some(line) = output.lines().find(|l| l.contains("brain URL is:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let url = parts[1].trim();
                    account_info["brain_url"] = json!(url);
                }
            }
            
            // Extract SSH key path
            if let Some(line) = output.lines().find(|l| l.contains("SSH Key Path:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let path = parts[1].trim();
                    account_info["ssh_key_path"] = json!(path);
                }
            }
            
            // Extract wallet public key
            if let Some(line) = output.lines().find(|l| l.contains("Wallet public key:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let key = parts[1].trim();
                    account_info["wallet_public_key"] = json!(key);
                }
            }
            
            // Extract account balance
            if let Some(line) = output.lines().find(|l| l.contains("Account Balance:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let balance = parts[1].trim();
                    account_info["account_balance"] = json!(balance);
                }
            }
            
            // Extract wallet secret key path
            if let Some(line) = output.lines().find(|l| l.contains("Wallet secret key path:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let path = parts[1].trim();
                    account_info["wallet_secret_key_path"] = json!(path);
                }
            }
            
            return Ok(account_info);
        }
        
        // Check for VM creation output
        if output.contains("VM CREATED") {
            let mut vm_info = json!({});
            
            // Extract hostname
            if let Some(line) = output.lines().find(|l| l.contains("Using random VM name:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    vm_info["hostname"] = json!(parts[1].trim());
                }
            }
            
            // Extract price
            if let Some(line) = output.lines().find(|l| l.contains("Node price:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let price_parts: Vec<&str> = parts[1].split('/').collect();
                    vm_info["price"] = json!(price_parts[0].trim());
                }
            }
            
            // Extract total units
            if let Some(line) = output.lines().find(|l| l.contains("Total Units for hardware requested:")) {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    if let Ok(units) = parts[1].trim().parse::<i64>() {
                        vm_info["total_units"] = json!(units);
                    }
                }
            }
            
            // Extract locked LP
            if let Some(line) = output.lines().find(|l| l.contains("Locking")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(lp) = parts[1].parse::<f64>() {
                        vm_info["locked_lp"] = json!(lp);
                    }
                }
            }
            
            // Extract SSH info
            if let Some(line) = output.lines().find(|l| l.contains("ssh -p")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    // Format: ssh -p PORT root@HOST
                    if let Ok(port) = parts[2].parse::<i64>() {
                        vm_info["ssh_port"] = json!(port);
                    }
                    
                    let host_part = parts[3];
                    let host = host_part.split('@').nth(1).unwrap_or("");
                    vm_info["ssh_host"] = json!(host);
                }
            }
            
            // Extract UUID
            if let Some(line) = output.lines().find(|l| l.contains("VM CREATED")) {
                // Use a simple pattern to extract UUID
                let uuid_pattern = "VM CREATED!";
                if let Some(idx) = line.find(uuid_pattern) {
                    let rest = &line[idx + uuid_pattern.len()..];
                    let uuid_re = regex::Regex::new(r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})").unwrap();
                    if let Some(caps) = uuid_re.captures(rest) {
                        vm_info["uuid"] = json!(caps.get(1).unwrap().as_str());
                    }
                }
            }
            
            return Ok(vm_info);
        }
        
        // Check for VM list output
        if output.contains("| City") && output.contains("| UUID") {
            let workers = self.parse_workers_table(output);
            return Ok(json!(workers));
        }
        
        // Check for VM update output
        if output.contains("hardware modifications") || output.contains("will run for another") {
            let mut update_info = json!({
                "success": true
            });
            
            // Extract hardware modification status
            if output.contains("The node accepted the hardware modifications for the VM") {
                update_info["hardware_modified"] = json!(true);
            }
            
            // Extract hours updated
            if let Some(line) = output.lines().find(|l| l.contains("The VM will run for another")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 7 {
                    if let Ok(hours) = parts[6].parse::<i64>() {
                        update_info["hours_updated"] = json!(hours);
                    }
                }
            }
            
            return Ok(update_info);
        }
        
        // For any other output, just return a success flag
        Ok(json!({
            "success": true
        }))
    }
    
    // Implementation of individual actions
    
    fn test_install(&self) -> ActionResult {
        let output = self.run_detee_cmd("detee-cli --version")?;
        
        let result = self.cli_output_to_json(&output, &PathBuf::new())?;
        
        Ok(result)
    }
    
    fn setup_container(&self) -> ActionResult {
        let command = "docker run --pull always -dt --name detee-cli --volume ~/.detee/container_volume/cli:/root/.detee/cli:rw --volume ~/.detee/container_volume/.ssh:/root/.ssh:rw --entrypoint /usr/bin/fish detee/detee-cli:latest";
        
        let output = self.run_shell_cmd(command)?;
        
        let result = self.cli_output_to_json(&output, &PathBuf::new())?;
        
        Ok(json!({
            "success": true,
            "container_id": result["container_id"]
        }))
    }
    
    fn setup_account(&self) -> ActionResult {
        let command = "bash -c 'if [ ! -f /root/.ssh/id_ed25519.pub ]; then ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N \"}\" && detee-cli account ssh-pubkey-path /root/.ssh/id_ed25519.pub && detee-cli account brain-url http://164.92.249.180:31337'";
        
        let _ = self.run_detee_cmd(command)?;
        
        Ok(json!({
            "success": true
        }))
    }
    
    fn get_account_info(&self) -> ActionResult {
        let output = self.run_detee_cmd("detee-cli account")?;
        
        let account_info = self.cli_output_to_json(&output, &PathBuf::new())?;
        
        Ok(account_info)
    }
    
    fn create_worker(&self, distro: String, vcpus: i64, memory_mb: i64, disk_gb: i64, hours: i64) -> ActionResult {
        let command = format!(
            "detee-cli vm deploy --distro {} --vcpus {} --memory {} --disk {} --hours {}",
            distro, vcpus, memory_mb, disk_gb, hours
        );
        
        let output = self.run_detee_cmd(&command)?;
        
        let vm_info = self.cli_output_to_json(&output, &PathBuf::new())?;
        
        Ok(vm_info)
    }
    
    fn list_workers(&self) -> ActionResult {
        let output = self.run_detee_cmd("detee-cli vm list")?;
        
        let workers = self.cli_output_to_json(&output, &PathBuf::new())?;
        
        Ok(json!({
            "workers": workers
        }))
    }
    
    fn get_worker(&self, worker_id: String) -> ActionResult {
        let command = format!("detee-cli vm list | grep {}", worker_id);
        
        let output = self.run_detee_cmd(&command)?;
        
        if output.trim().is_empty() {
            return Err(format!("Worker with ID {} not found", worker_id));
        }
        
        // Parse the single VM line
        let workers = self.parse_workers_table(&output);
        
        if let Some(worker) = workers.first() {
            let vm_info = json!({
                "city": worker.city,
                "hostname": worker.hostname,
                "cores": worker.cores,
                "memory_mb": worker.memory_mb,
                "disk_gb": worker.disk_gb,
                "lp_per_hour": worker.lp_per_hour,
                "time_left": worker.time_left
            });
            
            Ok(json!({
                "vm": vm_info
            }))
        } else {
            Err(format!("Failed to parse worker info for ID {}", worker_id))
        }
    }
    
    fn has_worker(&self, worker_id: String) -> ActionResult {
        let command = format!("detee-cli vm list | grep {}", worker_id);
        
        let result = self.run_detee_cmd(&command);
        
        match result {
            Ok(output) => {
                let exists = !output.trim().is_empty();
                
                Ok(json!({
                    "success": true,
                    "exists": exists
                }))
            },
            Err(_) => {
                // If the command fails, the worker likely doesn't exist
                Ok(json!({
                    "success": true,
                    "exists": false
                }))
            }
        }
    }
    
    fn update_worker(&self, worker_id: String, vcpus_param: String, memory_param: String, hours_param: String) -> ActionResult {
        let command = format!(
            "detee-cli vm update {} {} {} {}",
            vcpus_param, memory_param, hours_param, worker_id
        );
        
        let output = self.run_detee_cmd(&command)?;
        
        let update_info = self.cli_output_to_json(&output, &PathBuf::new())?;
        
        Ok(update_info)
    }
    
    fn delete_worker(&self, worker_id: String) -> ActionResult {
        let command = format!("detee-cli vm delete {}", worker_id);
        
        let _ = self.run_detee_cmd(&command)?;
        
        Ok(json!({
            "success": true
        }))
    }
}

impl CpiExtension for DeeTeeExtension {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn provider_type(&self) -> &str {
        &self.provider_type
    }
    
    fn list_actions(&self) -> Vec<String> {
        vec![
            "test_install".to_string(),
            "setup_container".to_string(),
            "setup_account".to_string(),
            "get_account_info".to_string(),
            "create_worker".to_string(),
            "list_workers".to_string(),
            "get_worker".to_string(),
            "has_worker".to_string(),
            "update_worker".to_string(),
            "delete_worker".to_string(),
        ]
    }
    
    fn get_action_definition(&self, action: &str) -> Option<ActionDefinition> {
        match action {
            "test_install" => Some(ActionDefinition {
                name: "test_install".to_string(),
                description: "Test if DeeTEE CLI is properly installed in the container".to_string(),
                parameters: vec![],
            }),
            "setup_container" => Some(ActionDefinition {
                name: "setup_container".to_string(),
                description: "Setup the DeeTEE CLI container".to_string(),
                parameters: vec![],
            }),
            "setup_account" => Some(ActionDefinition {
                name: "setup_account".to_string(),
                description: "Setup the DeeTEE account with SSH key and brain URL".to_string(),
                parameters: vec![],
            }),
            "get_account_info" => Some(ActionDefinition {
                name: "get_account_info".to_string(),
                description: "Get DeeTEE account information".to_string(),
                parameters: vec![],
            }),
            "create_worker" => Some(ActionDefinition {
                name: "create_worker".to_string(),
                description: "Create a new DeeTEE virtual machine".to_string(),
                parameters: vec![
                    param!("distro", "Linux distribution", ParamType::String, optional, json!("ubuntu")),
                    param!("vcpus", "Number of vCPUs", ParamType::Integer, optional, json!(2)),
                    param!("memory_mb", "Memory in MB", ParamType::Integer, optional, json!(2048)),
                    param!("disk_gb", "Disk size in GB", ParamType::Integer, optional, json!(20)),
                    param!("hours", "Runtime in hours", ParamType::Integer, optional, json!(4)),
                ],
            }),
            "list_workers" => Some(ActionDefinition {
                name: "list_workers".to_string(),
                description: "List all DeeTEE virtual machines".to_string(),
                parameters: vec![],
            }),
            "get_worker" => Some(ActionDefinition {
                name: "get_worker".to_string(),
                description: "Get information about a DeeTEE virtual machine".to_string(),
                parameters: vec![
                    param!("worker_id", "UUID of the VM", ParamType::String, required),
                ],
            }),
            "has_worker" => Some(ActionDefinition {
                name: "has_worker".to_string(),
                description: "Check if a DeeTEE virtual machine exists".to_string(),
                parameters: vec![
                    param!("worker_id", "UUID of the VM", ParamType::String, required),
                ],
            }),
            "update_worker" => Some(ActionDefinition {
                name: "update_worker".to_string(),
                description: "Update a DeeTEE virtual machine".to_string(),
                parameters: vec![
                    param!("worker_id", "UUID of the VM", ParamType::String, required),
                    param!("vcpus_param", "vCPUs parameter string", ParamType::String, required),
                    param!("memory_param", "Memory parameter string", ParamType::String, required),
                    param!("hours_param", "Hours parameter string", ParamType::String, required),
                ],
            }),
            "delete_worker" => Some(ActionDefinition {
                name: "delete_worker".to_string(),
                description: "Delete a DeeTEE virtual machine".to_string(),
                parameters: vec![
                    param!("worker_id", "UUID of the VM", ParamType::String, required),
                ],
            }),
            _ => None,
        }
    }
    
    fn execute_action(&self, action: &str, params: &HashMap<String, Value>) -> ActionResult {
        match action {
            "test_install" => self.test_install(),
            "setup_container" => self.setup_container(),
            "setup_account" => self.setup_account(),
            "get_account_info" => self.get_account_info(),
            "create_worker" => {
                let distro = validation::extract_string_opt(params, "distro")?.unwrap_or_else(|| "ubuntu".to_string());
                let vcpus = validation::extract_int_opt(params, "vcpus")?.unwrap_or(2);
                let memory_mb = validation::extract_int_opt(params, "memory_mb")?.unwrap_or(2048);
                let disk_gb = validation::extract_int_opt(params, "disk_gb")?.unwrap_or(20);
                let hours = validation::extract_int_opt(params, "hours")?.unwrap_or(4);
                
                self.create_worker(distro, vcpus, memory_mb, disk_gb, hours)
            },
            "list_workers" => self.list_workers(),
            "get_worker" => {
                let worker_id = validation::extract_string(params, "worker_id")?;
                self.get_worker(worker_id)
            },
            "has_worker" => {
                let worker_id = validation::extract_string(params, "worker_id")?;
                self.has_worker(worker_id)
            },
            "update_worker" => {
                let worker_id = validation::extract_string(params, "worker_id")?;
                let vcpus_param = validation::extract_string(params, "vcpus_param")?;
                let memory_param = validation::extract_string(params, "memory_param")?;
                let hours_param = validation::extract_string(params, "hours_param")?;
                
                self.update_worker(worker_id, vcpus_param, memory_param, hours_param)
            },
            "delete_worker" => {
                let worker_id = validation::extract_string(params, "worker_id")?;
                self.delete_worker(worker_id)
            },
            _ => Err(format!("Action '{}' not found", action)),
        }
    }
}