use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
};

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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CustomService {
    id: Option<String>,
    name: String,
    port: Option<u16>,
    ports: Option<Vec<u16>>,
    cwd: Option<String>,
    path: Option<String>,
    start: String,
    stop: String,
    restart: Option<String>,
    status: Option<String>,
    logs: Option<String>,
    config: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CustomServiceInput {
    id: Option<String>,
    name: String,
    port: Option<u16>,
    cwd: Option<String>,
    path: Option<String>,
    start: String,
    stop: String,
    restart: Option<String>,
    status: Option<String>,
    logs: Option<String>,
    config: Option<String>,
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

    match service_ref.provider.as_str() {
        "docker" => {
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
        "custom" => return read_custom_service_config(&service_ref.name),
        _ => {}
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

    match service_ref.provider.as_str() {
        "docker" => return Err("This service config is read-only.".to_string()),
        "custom" => return save_custom_service_config(&service_ref.name, content),
        _ => {}
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
        "custom" => run_custom_start(&service_ref.name),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn stop_service(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker(&["stop", &service_ref.name]).map(|_| ()),
        "brew" => brew(&["services", "stop", &service_ref.name]).map(|_| ()),
        "custom" => run_custom_stop(&service_ref.name),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn restart_service(service: String) -> Result<(), String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker(&["restart", &service_ref.name]).map(|_| ()),
        "brew" => brew(&["services", "restart", &service_ref.name]).map(|_| ()),
        "custom" => run_custom_restart(&service_ref.name),
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
        "custom" => custom_service_path(&service_ref.name),
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
        "custom" => Ok(ServiceDetails {
            version: String::new(),
            latest_version: String::new(),
            config_path: custom_service_config_path(&service_ref.name).unwrap_or_default(),
            path: custom_service_path(&service_ref.name).unwrap_or_default(),
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
        "custom" => custom_service_logs(&service_ref.name),
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn service_status(service: String) -> Result<String, String> {
    let service_ref = parse_service_id(&service)?;
    match service_ref.provider.as_str() {
        "docker" => docker_status(&service_ref.name),
        "brew" => brew_status(&service_ref.name),
        "custom" => {
            custom_service(&service_ref.name).map(|service| custom_service_status(&service))
        }
        provider => Err(format!("Unsupported provider: {provider}")),
    }
}

#[tauri::command]
fn add_custom_service(service: CustomServiceInput) -> Result<(), String> {
    if service.name.trim().is_empty() {
        return Err("Custom service name is required.".to_string());
    }
    if service.start.trim().is_empty() {
        return Err("Custom service start command is required.".to_string());
    }
    if service.stop.trim().is_empty() {
        return Err("Custom service stop command is required.".to_string());
    }

    let next_service = CustomService {
        id: service.id.filter(|value| !value.trim().is_empty()),
        name: service.name.trim().to_string(),
        port: service.port,
        ports: None,
        cwd: clean_optional_string(service.cwd),
        path: clean_optional_string(service.path),
        start: service.start.trim().to_string(),
        stop: service.stop.trim().to_string(),
        restart: clean_optional_string(service.restart),
        status: clean_optional_string(service.status),
        logs: clean_optional_string(service.logs),
        config: clean_optional_string(service.config),
    };
    let next_id = custom_service_id(&next_service);
    if custom_service_file_path(&next_id)?.exists() {
        return Err(format!("Custom service already exists: {next_id}"));
    }

    write_custom_service(&next_id, &next_service)
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
            service_logs,
            service_status,
            add_custom_service
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
    if let Ok(mut custom_services) = discover_custom_services() {
        definitions.append(&mut custom_services);
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

fn discover_custom_services() -> Result<Vec<ServiceDefinition>, String> {
    Ok(read_custom_services()?
        .into_iter()
        .map(|service| {
            let id = custom_service_id(&service);
            let status = custom_service_status(&service);
            let mut ports = service.ports.clone().unwrap_or_default();
            if let Some(port) = service.port {
                push_unique_port(&mut ports, port);
            }

            ServiceDefinition {
                key: format!("custom:{id}"),
                provider: "custom".to_string(),
                name: service.name,
                status,
                ports,
                port_mappings: Vec::new(),
                path: service.path.or(service.cwd),
                image: Some(service.start),
                version: None,
                latest_version: None,
                user: None,
                config_path: service.config,
            }
        })
        .collect())
}

fn read_custom_services() -> Result<Vec<CustomService>, String> {
    migrate_legacy_custom_services()?;

    let dir = custom_services_config_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut services = Vec::new();
    for entry in
        fs::read_dir(&dir).map_err(|err| format!("Failed to read {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("Failed to read custom service entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let created_at = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.created().ok());
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
        let service: CustomService = serde_json::from_str(&content)
            .map_err(|err| format!("Invalid custom service JSON at {}: {err}", path.display()))?;
        services.push((created_at, service));
    }

    services.sort_by(
        |(left_created, left_service), (right_created, right_service)| {
            right_created
                .cmp(left_created)
                .then_with(|| right_service.name.cmp(&left_service.name))
        },
    );

    Ok(services
        .into_iter()
        .map(|(_created_at, service)| service)
        .collect())
}

fn write_custom_service(id: &str, service: &CustomService) -> Result<(), String> {
    let dir = custom_services_config_dir()?;
    fs::create_dir_all(&dir).map_err(|err| format!("Failed to create {}: {err}", dir.display()))?;
    let path = custom_service_file_path(id)?;
    let content = serde_json::to_string_pretty(service)
        .map_err(|err| format!("Failed to serialize custom service: {err}"))?;
    fs::write(&path, content).map_err(|err| format!("Failed to write {}: {err}", path.display()))
}

fn custom_services_config_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set.".to_string())?;
    Ok(PathBuf::from(home)
        .join("servicepilot")
        .join("customservices"))
}

fn custom_service_file_path(id: &str) -> Result<PathBuf, String> {
    Ok(custom_services_config_dir()?.join(format!("{}.json", safe_file_name(id))))
}

fn legacy_custom_services_config_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set.".to_string())?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Service Pilot")
        .join("custom-services.json"))
}

fn migrate_legacy_custom_services() -> Result<(), String> {
    let legacy_path = legacy_custom_services_config_path()?;
    if !legacy_path.exists() || custom_services_config_dir()?.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&legacy_path)
        .map_err(|err| format!("Failed to read {}: {err}", legacy_path.display()))?;
    let services: Vec<CustomService> = serde_json::from_str(&content).map_err(|err| {
        format!(
            "Invalid custom services JSON at {}: {err}",
            legacy_path.display()
        )
    })?;
    for service in services {
        write_custom_service(&custom_service_id(&service), &service)?;
    }
    Ok(())
}

fn custom_service(id: &str) -> Result<CustomService, String> {
    read_custom_services()?
        .into_iter()
        .find(|service| custom_service_id(service) == id)
        .ok_or_else(|| format!("Custom service not found: {id}"))
}

fn custom_service_id(service: &CustomService) -> String {
    service
        .id
        .clone()
        .unwrap_or_else(|| service.name.to_lowercase().replace(' ', "-"))
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn safe_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn custom_service_status(service: &CustomService) -> String {
    let Some(status) = service.status.as_deref() else {
        return "unknown".to_string();
    };
    if run_shell_status(status, service.cwd.as_deref()).unwrap_or(false) {
        "running".to_string()
    } else {
        "stopped".to_string()
    }
}

fn run_custom_start(id: &str) -> Result<(), String> {
    let service = custom_service(id)?;
    spawn_shell_command(&service.start, service.cwd.as_deref())
}

fn run_custom_stop(id: &str) -> Result<(), String> {
    let service = custom_service(id)?;
    run_shell_command(&service.stop, service.cwd.as_deref()).map(|_| ())
}

fn run_custom_restart(id: &str) -> Result<(), String> {
    let service = custom_service(id)?;
    if let Some(restart) = service
        .restart
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        return run_shell_command(restart, service.cwd.as_deref()).map(|_| ());
    }

    run_shell_command(&service.stop, service.cwd.as_deref())?;
    spawn_shell_command(&service.start, service.cwd.as_deref())
}

fn custom_service_logs(id: &str) -> Result<String, String> {
    let service = custom_service(id)?;
    if let Some(logs) = service
        .logs
        .as_deref()
        .filter(|command| !command.trim().is_empty())
    {
        return run_shell_command(logs, service.cwd.as_deref());
    }
    serde_json::to_string_pretty(&service)
        .map_err(|err| format!("Failed to render custom service definition: {err}"))
}

fn read_custom_service_config(id: &str) -> Result<ServiceConfig, String> {
    let service = custom_service(id)?;
    let Some(path) = service.config else {
        let content = serde_json::to_string_pretty(&service)
            .map_err(|err| format!("Failed to render custom service definition: {err}"))?;
        return Ok(ServiceConfig {
            path: custom_services_config_dir()?.display().to_string(),
            content,
            readonly: true,
            message: "No config path was configured for this custom service.".to_string(),
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

fn save_custom_service_config(id: &str, content: String) -> Result<(), String> {
    let service = custom_service(id)?;
    let Some(path) = service.config else {
        return Err("No config path was configured for this custom service.".to_string());
    };
    fs::write(&path, content).map_err(|err| format!("Failed to write {path}: {err}"))
}

fn custom_service_config_path(id: &str) -> Option<String> {
    custom_service(id).ok().and_then(|service| service.config)
}

fn custom_service_path(id: &str) -> Option<String> {
    custom_service(id)
        .ok()
        .and_then(|service| service.path.or(service.cwd))
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

fn brew_status(service: &str) -> Result<String, String> {
    let services = discover_brew_services()?;
    services
        .into_iter()
        .find(|definition| definition.key == format!("brew:{service}"))
        .map(|definition| definition.status)
        .ok_or_else(|| format!("Homebrew service not found: {service}"))
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

fn docker_status(container: &str) -> Result<String, String> {
    let status = docker(&["inspect", "--format", "{{.State.Status}}", container])?
        .trim()
        .to_string();
    if status.is_empty() {
        Err(format!("Docker container status is empty: {container}"))
    } else {
        Ok(status)
    }
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

fn run_shell_command(command_text: &str, cwd: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("/bin/zsh");
    command.arg("-lc").arg(command_text);
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        command.current_dir(cwd);
    }

    let output = command
        .output()
        .map_err(|err| format!("Failed to run custom command: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(stdout)
    } else if stderr.is_empty() {
        Err(format!("Custom command failed: {command_text}"))
    } else {
        Err(stderr)
    }
}

fn spawn_shell_command(command_text: &str, cwd: Option<&str>) -> Result<(), String> {
    let mut command = Command::new("/bin/zsh");
    command
        .arg("-lc")
        .arg(command_text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        command.current_dir(cwd);
    }

    command
        .spawn()
        .map_err(|err| format!("Failed to start custom command: {err}"))?;
    Ok(())
}

fn run_shell_status(command_text: &str, cwd: Option<&str>) -> Result<bool, String> {
    let mut command = Command::new("/bin/zsh");
    command
        .arg("-lc")
        .arg(command_text)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
        command.current_dir(cwd);
    }

    let status = command
        .status()
        .map_err(|err| format!("Failed to run custom status command: {err}"))?;
    Ok(status.success())
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
