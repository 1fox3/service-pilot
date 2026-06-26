use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command, thread};

#[derive(Debug, Clone)]
struct ServiceDefinition {
    key: String,
    provider: String,
    name: String,
    status: String,
    ports: Vec<u16>,
    port_mappings: Vec<String>,
    path: Option<String>,
    image: Option<String>,
    version: Option<String>,
    latest_version: Option<String>,
    user: Option<String>,
    config_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrewService {
    name: String,
    status: String,
    user: Option<String>,
    file: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerContainer {
    names: String,
    state: String,
    ports: String,
    image: String,
    labels: String,
    mounts: String,
}

#[derive(Debug, Serialize)]
struct ServiceState {
    id: String,
    provider: String,
    name: String,
    status: String,
    port: u16,
    ports: Vec<u16>,
    port_mappings: Vec<String>,
    path: String,
    image: String,
    version: String,
    latest_version: String,
    user: String,
    config_path: String,
}

#[derive(Debug, Serialize)]
struct ServiceConfig {
    path: String,
    content: String,
    readonly: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct ServiceDetails {
    version: String,
    latest_version: String,
    config_path: String,
    path: String,
}

#[derive(Debug)]
struct ServiceRef {
    provider: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct BrewInfo {
    formulae: Vec<BrewFormula>,
}

#[derive(Debug, Deserialize)]
struct BrewFormula {
    installed: Vec<BrewInstalled>,
    versions: BrewVersions,
}

#[derive(Debug, Deserialize)]
struct BrewInstalled {
    version: String,
}

#[derive(Debug, Deserialize)]
struct BrewVersions {
    stable: Option<String>,
}

#[tauri::command]
fn list_services() -> Result<Vec<ServiceState>, String> {
    Ok(discover_services()
        .into_iter()
        .map(|service| service.into_state())
        .collect())
}

#[tauri::command]
fn read_service_config(service: String) -> Result<ServiceConfig, String> {
    let service_ref = parse_service_id(&service)?;

    if service_ref.provider == "docker" {
        let content = docker(&["inspect", &service_ref.name])?;
        return Ok(ServiceConfig {
            path: format!("docker inspect {}", service_ref.name),
            content,
            readonly: true,
            message:
                "Docker container configuration is shown from docker inspect and is read-only."
                    .to_string(),
        });
    }

    let Some(path) = brew_config_path(&service_ref.name) else {
        return Ok(ServiceConfig {
            path: String::new(),
            content: String::new(),
            readonly: true,
            message: "No editable config file was discovered for this service.".to_string(),
        });
    };

    let content =
        fs::read_to_string(&path).map_err(|err| format!("Failed to read {path}: {err}"))?;
    Ok(ServiceConfig {
        path,
        content,
        readonly: false,
        message: "Config file loaded.".to_string(),
    })
}

#[tauri::command]
fn save_service_config(service: String, content: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;

    if service_ref.provider == "docker" {
        return Err("This service config is read-only.".to_string());
    }

    let Some(path) = brew_config_path(&service_ref.name) else {
        return Err("No editable config file was discovered for this service.".to_string());
    };

    fs::write(&path, content).map_err(|err| format!("Failed to write {path}: {err}"))
}

#[tauri::command]
fn start_service(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker(&["start", &service_ref.name]).map(|_| ()),
        "brew" => brew(&["services", "start", &service_ref.name]).map(|_| ()),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn stop_service(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker(&["stop", &service_ref.name]).map(|_| ()),
        "brew" => brew(&["services", "stop", &service_ref.name]).map(|_| ()),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn restart_service(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker(&["restart", &service_ref.name]).map(|_| ()),
        "brew" => brew(&["services", "restart", &service_ref.name]).map(|_| ()),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn open_service_path(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    let path = match service_ref.provider.as_str() {
        "brew" => brew_install_path(&service_ref.name),
        "docker" => docker_mounts(&service_ref.name)
            .ok()
            .and_then(|path| openable_path(&path)),
        provider => return Err(format!("Unsupported provider: {provider}")),
    };
    let Some(path) = path else {
        return Err("No path discovered for this service.".to_string());
    };

    command("open", &[&path]).map(|_| ())
}

#[tauri::command]
fn update_service(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "brew" => brew(&["upgrade", &service_ref.name]).map(|_| ()),
        provider => Err(format!("Update is not supported for provider: {provider}")),
    }
}

#[tauri::command]
fn service_details(service: String) -> Result<ServiceDetails, String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "brew" => {
            let (version, latest_version) = brew_versions(&service_ref.name).unwrap_or_default();
            Ok(ServiceDetails {
                version,
                latest_version,
                config_path: brew_config_path(&service_ref.name).unwrap_or_default(),
                path: brew_install_path(&service_ref.name).unwrap_or_default(),
            })
        }
        "docker" => Ok(ServiceDetails {
            version: String::new(),
            latest_version: String::new(),
            config_path: String::new(),
            path: docker_mounts(&service_ref.name).unwrap_or_default(),
        }),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn service_logs(service: String) -> Result<String, String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker(&["logs", "--tail", "160", &service_ref.name]),
        "brew" => brew(&["services", "info", &service_ref.name]),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_services,
            read_service_config,
            save_service_config,
            start_service,
            stop_service,
            restart_service,
            open_service_path,
            update_service,
            service_details,
            service_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

impl ServiceDefinition {
    fn into_state(self) -> ServiceState {
        let port = self.ports.first().copied().unwrap_or(0);

        ServiceState {
            id: self.key,
            provider: self.provider,
            name: self.name,
            status: self.status,
            port,
            ports: self.ports,
            port_mappings: self.port_mappings,
            path: self.path.unwrap_or_default(),
            image: self.image.unwrap_or_default(),
            version: self.version.unwrap_or_default(),
            latest_version: self.latest_version.unwrap_or_default(),
            user: self.user.unwrap_or_default(),
            config_path: self.config_path.unwrap_or_default(),
        }
    }
}

fn discover_services() -> Vec<ServiceDefinition> {
    let docker_handle = thread::spawn(discover_docker_containers);
    let brew_handle = thread::spawn(discover_brew_services);

    let mut definitions = docker_handle
        .join()
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    if let Some(mut brew_services) = brew_handle.join().ok().and_then(Result::ok) {
        definitions.append(&mut brew_services);
    }

    definitions.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));
    definitions
}

fn discover_docker_containers() -> Result<Vec<ServiceDefinition>, String> {
    let output = docker(&["ps", "-a", "--format", "json"])?;
    let mut definitions = Vec::new();

    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let container: DockerContainer =
            serde_json::from_str(line).map_err(|err| format!("Invalid docker ps JSON: {err}"))?;

        let name = container
            .names
            .split(',')
            .next()
            .unwrap_or(&container.names)
            .trim();
        if name.is_empty() {
            continue;
        }

        let port_mappings = docker_port_mappings(&container.ports)
            .or_else(|| docker_label_port_mappings(&container.labels))
            .or_else(|| docker_inspect_port_mappings(name).ok())
            .unwrap_or_default();
        let ports = if port_mappings.is_empty() {
            docker_ports(&container.ports)
                .or_else(|| docker_label_ports(&container.labels))
                .or_else(|| docker_inspect_ports(name).ok())
                .unwrap_or_default()
        } else {
            port_mappings
                .iter()
                .filter_map(|mapping| mapping.split_once(':'))
                .filter_map(|(host, _container)| host.parse::<u16>().ok())
                .collect()
        };

        definitions.push(ServiceDefinition {
            key: format!("docker:{name}"),
            provider: "docker".to_string(),
            name: title_case(name),
            status: container.state,
            ports,
            port_mappings,
            path: Some(container.mounts),
            image: Some(container.image),
            version: None,
            latest_version: None,
            user: None,
            config_path: None,
        });
    }

    Ok(definitions)
}

fn discover_brew_services() -> Result<Vec<ServiceDefinition>, String> {
    let output = brew(&["services", "list", "--json"])?;
    let services: Vec<BrewService> = serde_json::from_str(&output)
        .map_err(|err| format!("Invalid brew services JSON: {err}"))?;

    Ok(services
        .into_iter()
        .map(|service| ServiceDefinition {
            key: format!("brew:{}", service.name),
            provider: "brew".to_string(),
            name: title_case(&service.name),
            status: service.status,
            ports: known_brew_port(&service.name).into_iter().collect(),
            port_mappings: Vec::new(),
            path: None,
            image: None,
            version: None,
            latest_version: None,
            user: service.user,
            config_path: service.file,
        })
        .collect())
}

fn brew_config_path(service: &str) -> Option<String> {
    let homebrew_prefix = homebrew_prefix();
    let candidates = match service {
        "grafana" => vec![format!("{homebrew_prefix}/etc/grafana/grafana.ini")],
        "nginx" => vec![format!("{homebrew_prefix}/etc/nginx/nginx.conf")],
        "prometheus" => vec![format!("{homebrew_prefix}/etc/prometheus.yml")],
        "redis" => vec![format!("{homebrew_prefix}/etc/redis.conf")],
        "mysql@5.7" => vec![format!("{homebrew_prefix}/etc/my.cnf")],
        _ => vec![],
    };

    candidates
        .into_iter()
        .find(|path| PathBuf::from(path).exists())
}

fn brew_install_path(service: &str) -> Option<String> {
    brew(&["--prefix", service])
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}

fn known_brew_port(service: &str) -> Option<u16> {
    match service {
        "grafana" => Some(3000),
        "jupyterlab" => Some(8888),
        "mysql@5.7" => Some(3306),
        "nginx" => Some(8080),
        "prometheus" => Some(9090),
        "redis" => Some(6379),
        _ => None,
    }
}

fn brew_versions(service: &str) -> Option<(String, String)> {
    let output = brew(&["info", "--json=v2", service]).ok()?;
    let info: BrewInfo = serde_json::from_str(&output).ok()?;
    let formula = info.formulae.first()?;
    let version = formula
        .installed
        .first()
        .map(|installed| installed.version.clone())
        .or_else(|| formula.versions.stable.clone())?;
    let latest_version = formula
        .versions
        .stable
        .clone()
        .unwrap_or_else(|| version.clone());

    Some((version, latest_version))
}

fn homebrew_prefix() -> String {
    std::env::var("HOMEBREW_PREFIX").unwrap_or_else(|_| "/opt/homebrew".to_string())
}

fn brew(args: &[&str]) -> Result<String, String> {
    command("brew", args)
}

fn docker(args: &[&str]) -> Result<String, String> {
    command("docker", args)
}

fn parse_service_id(service: &str) -> Result<ServiceRef, String> {
    let Some((provider, name)) = service.split_once(':') else {
        return Err(format!("Invalid service id: {service}"));
    };
    if provider.is_empty() || name.is_empty() {
        return Err(format!("Invalid service id: {service}"));
    }
    Ok(ServiceRef {
        provider: provider.to_string(),
        name: name.to_string(),
    })
}

fn docker_mounts(container: &str) -> Result<String, String> {
    let output = docker(&[
        "inspect",
        "--format",
        "{{range .Mounts}}{{.Source}},{{end}}",
        container,
    ])?;
    Ok(output.trim().trim_end_matches(',').to_string())
}

fn docker_ports(ports: &str) -> Option<Vec<u16>> {
    let mut result = Vec::new();
    for part in ports.split(',') {
        let part = part.trim();
        let Some((host, _container)) = part.split_once("->") else {
            continue;
        };
        let Some(port) = host.rsplit(':').next() else {
            continue;
        };
        if let Ok(port) = port.parse::<u16>() {
            push_unique_port(&mut result, port);
        }
    }

    (!result.is_empty()).then_some(result)
}

fn docker_port_mappings(ports: &str) -> Option<Vec<String>> {
    let mut result = Vec::new();
    for part in ports.split(',') {
        let part = part.trim();
        let Some((host, container)) = part.split_once("->") else {
            continue;
        };
        let Some(host_port) = host.rsplit(':').next() else {
            continue;
        };
        let Some(container_port) = container.split('/').next() else {
            continue;
        };
        if host_port.parse::<u16>().is_ok() && container_port.parse::<u16>().is_ok() {
            push_unique_mapping(&mut result, format!("{host_port}:{container_port}"));
        }
    }

    (!result.is_empty()).then_some(result)
}

fn docker_label_ports(labels: &str) -> Option<Vec<u16>> {
    docker_label_port_mappings(labels).map(|mappings| {
        mappings
            .iter()
            .filter_map(|mapping| mapping.split_once(':'))
            .filter_map(|(host, _container)| host.parse::<u16>().ok())
            .collect()
    })
}

fn docker_label_port_mappings(labels: &str) -> Option<Vec<String>> {
    let mut result = Vec::new();
    for label in labels.split(',') {
        let label = label.trim();
        let Some((key, value)) = label.split_once('=') else {
            continue;
        };
        let Some(container_port) = key
            .strip_prefix("desktop.docker.io/ports/")
            .and_then(|value| value.split('/').next())
        else {
            continue;
        };
        let host_port = value.trim_start_matches(':');
        if host_port.parse::<u16>().is_ok() && container_port.parse::<u16>().is_ok() {
            push_unique_mapping(&mut result, format!("{host_port}:{container_port}"));
        }
    }

    (!result.is_empty()).then_some(result)
}

fn docker_inspect_ports(container: &str) -> Result<Vec<u16>, String> {
    let output = docker(&["inspect", container])?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|err| format!("Invalid docker inspect JSON: {err}"))?;
    let Some(bindings) = value
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("HostConfig"))
        .and_then(|host_config| host_config.get("PortBindings"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };

    let mut result = Vec::new();
    for binding in bindings.values() {
        let Some(entries) = binding.as_array() else {
            continue;
        };
        for entry in entries {
            let Some(port) = entry
                .get("HostPort")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<u16>().ok())
            else {
                continue;
            };
            push_unique_port(&mut result, port);
        }
    }

    Ok(result)
}

fn docker_inspect_port_mappings(container: &str) -> Result<Vec<String>, String> {
    let output = docker(&["inspect", container])?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|err| format!("Invalid docker inspect JSON: {err}"))?;
    let Some(bindings) = value
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("HostConfig"))
        .and_then(|host_config| host_config.get("PortBindings"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };

    let mut result = Vec::new();
    for (container_port_key, binding) in bindings {
        let Some(container_port) = container_port_key.split('/').next() else {
            continue;
        };
        let Some(entries) = binding.as_array() else {
            continue;
        };
        for entry in entries {
            let Some(host_port) = entry.get("HostPort").and_then(Value::as_str) else {
                continue;
            };
            if host_port.parse::<u16>().is_ok() && container_port.parse::<u16>().is_ok() {
                push_unique_mapping(&mut result, format!("{host_port}:{container_port}"));
            }
        }
    }

    Ok(result)
}

fn push_unique_port(ports: &mut Vec<u16>, port: u16) {
    if !ports.contains(&port) {
        ports.push(port);
    }
}

fn push_unique_mapping(mappings: &mut Vec<String>, mapping: String) {
    if !mappings.contains(&mapping) {
        mappings.push(mapping);
    }
}

fn command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn openable_path(path: &str) -> Option<String> {
    if path.trim().is_empty() {
        return None;
    }

    for segment in path.split(',') {
        let candidate = segment.trim();
        if candidate.starts_with('/') {
            return Some(candidate.to_string());
        }
    }

    Some(path.trim().to_string())
}

fn title_case(value: &str) -> String {
    value
        .split(['-', '_', '@'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
