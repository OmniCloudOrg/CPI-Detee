# DeeTEE CPI Extension

This is a dynamic extension (DLL) implementation for managing DeeTEE virtual machines through the CPI (Cloud Provider Interface).

## Features

- Test DeeTEE CLI installation in the container
- Setup the DeeTEE container and account
- List, create, delete, and manage DeeTEE virtual machines

## Requirements

- Docker installed and running
- Rust programming environment
- DeeTEE CLI Docker image access

## Building

```bash
cargo build --release
```

The resulting DLL will be in `target/release/cpi_detee.dll` (Windows), `.so` (Linux), or `.dylib` (macOS).

## Installation

Copy the DLL to your application's extensions directory.

## Usage

This extension implements the following actions:

### Setup & Configuration
- `test_install`: Test if DeeTEE CLI is properly installed in the container
- `setup_container`: Setup the DeeTEE CLI container
- `setup_account`: Setup the DeeTEE account with SSH key and brain URL
- `get_account_info`: Get DeeTEE account information

### VM Management
- `create_worker`: Create a new DeeTEE virtual machine
- `list_workers`: List all DeeTEE virtual machines
- `get_worker`: Get information about a DeeTEE virtual machine
- `has_worker`: Check if a DeeTEE virtual machine exists
- `update_worker`: Update a DeeTEE virtual machine
- `delete_worker`: Delete a DeeTEE virtual machine

## Technical Details

This extension uses Docker commands to interact with the DeeTEE CLI container. All operations are performed by executing commands via the `docker exec` API in Rust. The extension parses the output of DeeTEE CLI commands and maps them to structured JSON responses compatible with the CPI interface.

### Implementation Notes

- The extension uses Rust structs with Serde for mapping CLI outputs to structured data
- Docker is required for running the DeeTEE CLI container
- All actions maintain the same response format as other CPI providers
- The `id` field in responses uses the UUID assigned by DeeTEE

### DeeTEE Container Management

The extension manages the DeeTEE CLI container in these ways:
1. `setup_container`: Creates and starts the DeeTEE container
2. All subsequent commands execute inside this container
3. Volume mounts are set up for persisting configuration and SSH keys

### VM Parameters

When creating virtual machines, the following parameters can be specified:
- `distro`: Linux distribution (default: "ubuntu")
- `vcpus`: Number of vCPUs (default: 2)
- `memory_mb`: Memory in MB (default: 2048)
- `disk_gb`: Disk size in GB (default: 20)
- `hours`: Runtime in hours (default: 4)

## Error Handling

The extension captures and processes errors from DeeTEE CLI commands and Docker operations, returning structured error messages for proper error handling in the CPI system.

## Working with Update Parameters

The `update_worker` action requires specific parameter strings:
- `vcpus_param`: Format as `--vcpus NUMBER` or empty string to keep current value
- `memory_param`: Format as `--memory NUMBER` or empty string to keep current value
- `hours_param`: Format as `--hours NUMBER` or empty string to keep current value

Example:
```rust
// To update only memory and hours:
let params = HashMap::from([
    ("worker_id", json!("uuid-here")),
    ("vcpus_param", json!("")),  // Keep current vCPUs
    ("memory_param", json!("--memory 4096")),  // Update to 4 GB
    ("hours_param", json!("--hours 12")),  // Extend by 12 hours
]);
```

## Data Mapping

The extension maps DeeTEE CLI output to structured JSON responses. For example, a VM creation response includes:
- hostname: The name of the VM
- price: The price per unit
- total_units: Total hardware units
- locked_lp: Amount of LP locked for the VM
- ssh_port: SSH port for connecting
- ssh_host: SSH host address
- uuid: Unique identifier for the VM

## Security Considerations

Since this extension executes Docker commands, it requires appropriate permissions. Ensure that the user running the application has Docker permissions.

## Cross-Platform Support

This extension is designed to work on both Windows and Unix-based systems:

- **Windows**: Uses CMD or PowerShell when appropriate
- **Unix**: Uses sh with bash commands

The extension automatically detects the platform and adjusts commands and paths accordingly:

```rust
if cfg!(windows) {
    // Windows-specific command
    let command = "cmd /C docker ...";
} else {
    // Unix-specific command
    let command = "docker ...";
}
```

### Windows-Specific Details

On Windows:
- Container volume paths use Windows environment variables (%USERPROFILE%)
- Directory creation uses PowerShell
- Commands are executed through cmd.exe
- Escaping is handled specifically for Windows command shell

### Unix-Specific Details

On Unix systems:
- Container volume paths use Unix home directory expansion (~/)
- Directory creation uses standard mkdir
- Commands are executed directly
- Escaping follows Unix shell patterns