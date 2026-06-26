import React, { useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type ServiceState = {
  id: string;
  provider: "docker" | "brew" | string;
  name: string;
  status: string;
  port: number;
  ports: number[];
  port_mappings: string[];
  path: string;
  image: string;
  version: string;
  user: string;
  config_path: string;
};

type ServiceConfig = {
  path: string;
  content: string;
  readonly: boolean;
  message: string;
};

type ServiceDetails = {
  version: string;
  config_path: string;
  path: string;
};

const BASIC_SWITCH_LOADING_MS = 120;

type RefreshProgress = {
  active: boolean;
  label: string;
  current: number;
  total: number;
};

const idleRefreshProgress: RefreshProgress = {
  active: false,
  label: "Ready",
  current: 0,
  total: 0
};

function App() {
  const [services, setServices] = useState<ServiceState[]>([]);
  const [selectedService, setSelectedService] = useState("");
  const [activeService, setActiveService] = useState("");
  const [logs, setLogs] = useState("");
  const [config, setConfig] = useState<ServiceConfig | null>(null);
  const [configContent, setConfigContent] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [loadingServices, setLoadingServices] = useState(true);
  const [loadingConfig, setLoadingConfig] = useState(false);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [loadingServiceSwitch, setLoadingServiceSwitch] = useState(false);
  const [message, setMessage] = useState("Ready");
  const [collapsedProviders, setCollapsedProviders] = useState<Record<string, boolean>>({});
  const [activePanel, setActivePanel] = useState<"config" | "logs">("config");
  const [hydratingService, setHydratingService] = useState<string | null>(null);
  const [refreshProgress, setRefreshProgress] = useState<RefreshProgress>(idleRefreshProgress);
  const switchToken = useRef(0);
  const switchTimer = useRef<number | null>(null);
  const serviceDetailsCache = useRef(new Map<string, ServiceDetails>());

  const selected = services.find((service) => service.id === selectedService) ?? services[0];
  const activeServiceId = activeService || selectedService || selected?.id;
  const groupedServices = services.reduce<Record<string, ServiceState[]>>((groups, service) => {
    groups[service.provider] = [...(groups[service.provider] ?? []), service];
    return groups;
  }, {});

  async function refresh() {
    flushSync(() => {
      setLoadingServices(true);
      setRefreshProgress({ active: true, label: "Preparing refresh", current: 0, total: 1 });
    });
    await nextPaint();

    try {
      setRefreshProgress({ active: true, label: "Discovering services", current: 1, total: 3 });
      const serviceList = await invoke<ServiceState[]>("list_services");
      const hydratedServices = await preloadBrewDetails(serviceList, true);
      setRefreshProgress({ active: true, label: "Applying service details", current: 3, total: 3 });
      setServices(hydratedServices);
      if (serviceList.length > 0 && !serviceList.some((service) => service.id === selectedService)) {
        const nextService = serviceList[0].id;
        setSelectedService(nextService);
        setActiveService(nextService);
        loadConfigForService(nextService);
      }
    } finally {
      window.setTimeout(() => {
        setLoadingServices(false);
        setRefreshProgress(idleRefreshProgress);
      }, 120);
    }
  }

  async function preloadBrewDetails(serviceList: ServiceState[], forceRefresh = false) {
    const brewServices = serviceList.filter((service) => service.provider === "brew");
    const detailsByService = new Map<string, ServiceDetails>();

    if (brewServices.length === 0) {
      setRefreshProgress({ active: true, label: "No Homebrew details to load", current: 2, total: 3 });
      return serviceList;
    }

    for (const [index, service] of brewServices.entries()) {
      const detailProgress = 1 + ((index + 1) / brewServices.length);
      setRefreshProgress({
        active: true,
        label: `Loading Homebrew details for ${service.name}`,
        current: detailProgress,
        total: 3
      });

      const cachedDetails = serviceDetailsCache.current.get(service.id);
      if (cachedDetails && !forceRefresh) {
        detailsByService.set(service.id, cachedDetails);
      } else {
        try {
          const details = await invoke<ServiceDetails>("service_details", { service: service.id });
          serviceDetailsCache.current.set(service.id, details);
          detailsByService.set(service.id, details);
        } catch (error) {
          setMessage(String(error));
        }
      }
    }

    setRefreshProgress({
      active: true,
      label: "Applying service details",
      current: 2,
      total: 3
    });

    return serviceList.map((service) => {
      const details = detailsByService.get(service.id);
      return details ? applyServiceDetails(service, details) : service;
    });
  }

  function nextPaint() {
    return new Promise<void>((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    });
  }

  async function runAction(action: "start" | "stop" | "restart", service: string) {
    setBusy(`${action}:${service}`);
    setMessage(`${action} ${service}...`);
    try {
      await invoke(`${action}_service`, { service });
      await refresh();
      setMessage(`${service} ${action} complete`);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(null);
    }
  }

  async function runUpdate(service: string) {
    setBusy(`update:${service}`);
    setMessage(`update ${service}...`);
    try {
      await invoke("update_service", { service });
      serviceDetailsCache.current.delete(service);
      await refresh();
      setMessage(`${service} update complete`);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(null);
    }
  }

  async function openServicePath(service: string) {
    setBusy(`open:${service}`);
    try {
      await invoke("open_service_path", { service });
      setMessage("Opened service folder in Finder.");
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(null);
    }
  }

  async function loadLogs(service = selected?.id) {
    if (!service) {
      return;
    }

    setBusy(`logs:${service}`);
    setLoadingLogs(true);
    setActiveService(service);
    setSelectedService(service);
    try {
      const output = await invoke<string>("service_logs", { service });
      setLogs(output || "No logs yet.");
      setMessage(`Loaded service info for ${service}`);
    } catch (error) {
      setLogs("");
      setMessage(String(error));
    } finally {
      setLoadingLogs(false);
      setBusy(null);
    }
  }

  async function loadConfig(service = selected?.id) {
    if (!service) {
      return;
    }

    await loadConfigForService(service);
  }

  async function saveConfig() {
    if (!selected || !config || config.readonly) {
      return;
    }

    setBusy(`save-config:${selected.id}`);
    setLoadingConfig(true);
    try {
      await invoke("save_service_config", { service: selected.id, content: configContent });
      setMessage("Config saved. Restart the service to apply changes if needed.");
      await loadConfig(selected.id);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setLoadingConfig(false);
      setBusy(null);
    }
  }

  async function selectService(serviceId: string) {
    if (serviceId === selectedService) {
      return;
    }

    const service = services.find((service) => service.id === serviceId);
    const token = switchToken.current + 1;
    switchToken.current = token;
    if (switchTimer.current !== null) {
      window.clearTimeout(switchTimer.current);
    }

    flushSync(() => {
      setActiveService(serviceId);
      setLoadingServiceSwitch(true);
    });

    switchTimer.current = window.setTimeout(() => {
      if (switchToken.current !== token) {
        return;
      }

      setSelectedService(serviceId);
      setConfig(null);
      setConfigContent("");
      setLogs("");
      setMessage("Ready");
      setHydratingService(service?.provider === "brew" ? serviceId : null);
      setLoadingServiceSwitch(false);
      switchTimer.current = null;
      loadConfigForService(serviceId, token);

      if (service?.provider === "brew") {
        window.setTimeout(() => {
          if (switchToken.current === token) {
            loadServiceDetails(serviceId, service, token);
          }
        }, 0);
      }
    }, BASIC_SWITCH_LOADING_MS);
  }

  async function loadServiceDetails(serviceId: string, service?: ServiceState, token = switchToken.current) {
    if (!service || service.provider !== "brew") {
      return;
    }

    const cachedDetails = serviceDetailsCache.current.get(serviceId);
    if (cachedDetails) {
      mergeServiceDetails(serviceId, cachedDetails);
      setHydratingService((current) => (current === serviceId ? null : current));
      return;
    }

    setHydratingService(serviceId);
    try {
      const details = await invoke<ServiceDetails>("service_details", { service: serviceId });
      if (switchToken.current !== token) {
        return;
      }
      serviceDetailsCache.current.set(serviceId, details);
      mergeServiceDetails(serviceId, details);
    } catch (error) {
      if (switchToken.current !== token) {
        return;
      }
      setMessage(String(error));
    } finally {
      if (switchToken.current === token) {
        setHydratingService((current) => (current === serviceId ? null : current));
      }
    }
  }

  async function loadConfigForService(service: string, token = switchToken.current) {
    setBusy(`config:${service}`);
    setLoadingConfig(true);
    try {
      const nextConfig = await invoke<ServiceConfig>("read_service_config", { service });
      if (switchToken.current !== token) {
        return;
      }
      setConfig(nextConfig);
      setConfigContent(nextConfig.content);
      setMessage(nextConfig.message);
    } catch (error) {
      if (switchToken.current !== token) {
        return;
      }
      setConfig(null);
      setConfigContent("");
      setMessage(String(error));
    } finally {
      if (switchToken.current === token) {
        setLoadingConfig(false);
        setBusy(null);
      }
    }
  }

  function mergeServiceDetails(serviceId: string, details: ServiceDetails) {
    setServices((current) =>
      current.map((service) =>
        service.id === serviceId ? applyServiceDetails(service, details) : service
      )
    );
  }

  function applyServiceDetails(service: ServiceState, details: ServiceDetails) {
    return {
      ...service,
      version: details.version || service.version,
      config_path: details.config_path || service.config_path,
      path: details.path || service.path
    };
  }

  function toggleProvider(provider: string) {
    setCollapsedProviders((current) => ({
      ...current,
      [provider]: !current[provider]
    }));
  }

  useEffect(() => {
    refresh().catch((error) => setMessage(String(error)));

    return () => {
      if (switchTimer.current !== null) {
        window.clearTimeout(switchTimer.current);
      }
    };
  }, []);

  return (
    <main className="appShell">
      {refreshProgress.active && <AppLoading progress={refreshProgress} />}
      <aside className="sidebar">
        <div className="brand">
          <p className="eyebrow">macOS Stack</p>
          <h1>Services</h1>
        </div>
        <button className="primary wide" onClick={refresh} disabled={busy !== null || loadingServices}>
          {loadingServices ? <LoadingText label="Refreshing" /> : "Refresh All"}
        </button>

        <nav className="serviceMenu">
          {loadingServices && services.length === 0 && <ServiceSkeleton />}
          {Object.entries(groupedServices).map(([provider, providerServices]) => (
            <section key={provider} className="providerGroup">
              <button
                className="providerHeader"
                onClick={() => toggleProvider(provider)}
                aria-expanded={!collapsedProviders[provider]}
              >
                <span>{providerLabel(provider)}</span>
                <span className="providerCount">{providerServices.length}</span>
                <span className={`chevron ${collapsedProviders[provider] ? "collapsed" : ""}`}>▾</span>
              </button>
              {!collapsedProviders[provider] && (
                <div className="serviceChildren">
                  {providerServices.map((service) => {
                    const running = isRunning(service.status);

                    return (
                      <div
                        key={service.id}
                        className={`serviceItem ${activeServiceId === service.id ? "active" : ""}`}
                      >
                        <button className="serviceSelect" onClick={() => selectService(service.id)}>
                          <span className={`dot ${running ? "ok" : "off"}`} />
                          <span className="serviceMain">
                            <span className="serviceName">{service.name}</span>
                            {servicePorts(service).length > 0 && (
                              <span className="servicePorts">
                                <span className="servicePortsLabel">Ports</span>
                                <span className="servicePortTabs">
                                  {servicePorts(service).map((port) => (
                                    <span className="servicePortTab" key={port}>{port}</span>
                                  ))}
                                </span>
                              </span>
                            )}
                          </span>
                        </button>
                        <div className="menuControls">
                          <button
                            className={`menuIconButton ${running ? "stop" : "start"}`}
                            onClick={() => runAction(running ? "stop" : "start", service.id)}
                            disabled={busy !== null}
                            title={running ? "Stop" : "Start"}
                            aria-label={`${running ? "Stop" : "Start"} ${service.name}`}
                          >
                            {busy === `${running ? "stop" : "start"}:${service.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true" />}
                          </button>
                          <button
                            className="menuIconButton restart"
                            onClick={() => runAction("restart", service.id)}
                            disabled={busy !== null}
                            title="Restart"
                            aria-label={`Restart ${service.name}`}
                          >
                            {busy === `restart:${service.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true">↻</span>}
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </section>
          ))}
        </nav>
      </aside>

      <section className="content">
        {loadingServiceSwitch && <PageLoading label="Loading service" />}
        {selected ? (
          <>
            <section className="serviceHeader panel">
              <div className="serviceHeroMain">
                <p className="eyebrow">{providerLabel(selected.provider)}</p>
                <div className="compactTitleRow">
                  <span className={`dot ${isRunning(selected.status) ? "ok" : "off"}`} />
                  <h2>{selected.name}</h2>
                  <span className="providerTag">{selected.status}</span>
                  <div className="headerActions">
                    <button className={`headerIconButton ${isRunning(selected.status) ? "stop" : "start"}`} onClick={() => runAction(isRunning(selected.status) ? "stop" : "start", selected.id)} disabled={busy !== null} title={isRunning(selected.status) ? "Stop" : "Start"} aria-label={isRunning(selected.status) ? "Stop service" : "Start service"}>
                      {busy === `${isRunning(selected.status) ? "stop" : "start"}:${selected.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true" />}
                    </button>
                    <button className="headerIconButton restart" onClick={() => runAction("restart", selected.id)} disabled={busy !== null} title="Restart" aria-label="Restart service">
                      {busy === `restart:${selected.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true">↻</span>}
                    </button>
                    {selected.provider === "brew" && (
                      <button className="headerIconButton update" onClick={() => runUpdate(selected.id)} disabled={busy !== null} title="Update" aria-label="Update service">
                        {busy === `update:${selected.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true">⇧</span>}
                      </button>
                    )}
                  </div>
                </div>
                <div className="compactMetaRow">
                  {selected.provider === "docker" ? (
                    <>
                      <Meta label="Image" value={selected.image || "n/a"} wide />
                      <Meta label="Port" value={rightPortValue(selected)} />
                    </>
                  ) : (
                    <>
                      <Meta label="Port" value={rightPortValue(selected)} />
                      <Meta
                        label="Path"
                        value={hydratingService === selected.id && !selected.path ? "Loading..." : selected.path || "n/a"}
                        wide
                        action={selected.path ? (
                          <button className="metaIconButton finder" onClick={() => openServicePath(selected.id)} disabled={busy !== null} title="Open in Finder" aria-label="Open service folder in Finder">
                            {busy === `open:${selected.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true">▰</span>}
                          </button>
                        ) : null}
                      />
                      <Meta label="User" value={selected.user || "n/a"} />
                      <Meta label="Version" value={hydratingService === selected.id && !selected.version ? "Loading..." : selected.version || "n/a"} />
                    </>
                  )}
                </div>
              </div>
            </section>

            <section className="panel tabPanel">
              <div className="tabHeader">
                <div className="tabs">
                  <button className={activePanel === "config" ? "active" : ""} onClick={() => setActivePanel("config")}>Config</button>
                  <button className={activePanel === "logs" ? "active" : ""} onClick={() => setActivePanel("logs")}>Logs</button>
                </div>
                {activePanel === "config" ? (
                  <div className="configActions">
                    <button className="panelIconButton reload" onClick={() => loadConfig(selected.id)} disabled={busy !== null || loadingConfig} title="Reload config" aria-label="Reload config">
                      {loadingConfig ? <span className="miniSpinner" /> : <span aria-hidden="true">↻</span>}
                    </button>
                    <button className="panelIconButton save" onClick={saveConfig} disabled={busy !== null || !config || config.readonly} title="Save config" aria-label="Save config">
                      {busy === `save-config:${selected.id}` ? <span className="miniSpinner" /> : <span aria-hidden="true">✓</span>}
                    </button>
                  </div>
                ) : (
                  <button className="panelIconButton reload" onClick={() => loadLogs(selected.id)} disabled={busy !== null || loadingLogs} title="Reload logs" aria-label="Reload logs">
                    {loadingLogs ? <span className="miniSpinner" /> : <span aria-hidden="true">↻</span>}
                  </button>
                )}
              </div>

              {activePanel === "config" ? (
                <div className="tabBody">
                  <p className="tabSubtext">{hydratingService === selected.id ? "Loading service details..." : config?.path || selected.config_path || "No config path discovered"}</p>
                  {config?.message && <p className="configMessage">{config.message}</p>}
                  <div className="editorWrap">
                    {loadingConfig && <LoadingOverlay label="Loading config" />}
                    <textarea
                      value={configContent}
                      onChange={(event) => setConfigContent(event.target.value)}
                      readOnly={!config || config.readonly || loadingConfig}
                      placeholder="No config loaded."
                    />
                  </div>
                </div>
              ) : (
                <div className="tabBody">
                  <div className="logWrap">
                    {loadingLogs && <LoadingOverlay label="Loading logs" />}
                    <pre>{logs || "Click Reload to load recent output."}</pre>
                  </div>
                </div>
              )}
            </section>
          </>
        ) : (
          <section className="panel emptyState">No service discovered.</section>
        )}

        <footer>{message}</footer>
      </section>
    </main>
  );
}

function LoadingText({ label }: { label: string }) {
  return (
    <span className="loadingText">
      <span className="miniSpinner" />
      {label}
    </span>
  );
}

function LoadingOverlay({ label }: { label: string }) {
  return (
    <div className="loadingOverlay">
      <span className="spinner" />
      <span>{label}</span>
    </div>
  );
}

function PageLoading({ label }: { label: string }) {
  return (
    <div className="pageLoading" aria-live="polite">
      <div className="pageLoadingCard">
        <span className="spinner" />
        <span>{label}</span>
      </div>
    </div>
  );
}

function AppLoading({ progress }: { progress: RefreshProgress }) {
  const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;

  return (
    <div className="appLoading" aria-live="polite" aria-busy="true">
      <div className="appLoadingCard">
        <div className="appLoadingTitle">
          <span className="spinner" />
          <div>
            <strong>Loading services</strong>
            <span>{progress.label}</span>
          </div>
        </div>
        <div className="progressTrack">
          <div className="progressFill" style={{ width: `${Math.min(100, Math.max(0, percent))}%` }} />
        </div>
        <p>{progress.total > 0 ? `${percent}%` : "Preparing"}</p>
      </div>
    </div>
  );
}

function ServiceSkeleton() {
  return (
    <div className="skeletonGroup">
      <div className="skeletonHeader" />
      {[0, 1, 2, 3].map((item) => (
        <div className="skeletonRow" key={item} />
      ))}
    </div>
  );
}

function Meta({ label, value, action, wide = false }: { label: string; value: string; action?: React.ReactNode; wide?: boolean }) {
  return (
    <div className={wide ? "metaItem metaItemLong" : "metaItem"}>
      <span>{label}</span>
      <div className={action ? "metaValueRow hasAction" : "metaValueRow"}>
        <strong className={value.includes("\n") ? "multiline" : ""}>{value}</strong>
        {action}
      </div>
    </div>
  );
}

function isRunning(status: string) {
  return status === "running" || status === "started";
}

function servicePorts(service: ServiceState) {
  return service.ports?.length ? service.ports : service.port > 0 ? [service.port] : [];
}

function rightPortValue(service: ServiceState) {
  if (service.provider === "docker" && service.port_mappings?.length) {
    return service.port_mappings.join("\n");
  }
  return servicePorts(service).length > 0 ? servicePorts(service).join(", ") : "n/a";
}

function providerLabel(provider: string) {
  if (provider === "brew") {
    return "Homebrew";
  }
  if (provider === "docker") {
    return "Docker Containers";
  }
  return provider;
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
