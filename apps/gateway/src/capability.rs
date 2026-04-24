//! Agent Capability Type-Safe System
//!
//! Provides a type-safe, structured capability system for agents that maps
//! to the kernel's CapabilitySet while maintaining serialization compatibility
//! for database storage and API interactions.
//!
//! 🔒 P1 FIX: Type-safe capability mapping from Gateway layer to Kernel layer.

use serde::{Deserialize, Serialize};

/// Type-safe agent capability definition
///
/// This enum provides structured capability definitions that can be
/// serialized to/from JSON for database storage and API interactions,
/// while also being convertible to kernel CapabilitySet for sandboxing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum AgentCapability {
    /// File system read access with specific paths
    #[serde(rename = "file_read")]
    FileRead { paths: Vec<String> },

    /// File system write access with specific paths
    #[serde(rename = "file_write")]
    FileWrite { paths: Vec<String> },

    /// HTTP network access to specific hosts
    #[serde(rename = "network_http")]
    NetworkHttp {
        hosts: Vec<String>,
        #[serde(default)]
        methods: Vec<HttpMethod>,
    },

    /// TCP network access to specific ports
    #[serde(rename = "network_tcp")]
    NetworkTcp {
        ports: Vec<u16>,
        #[serde(default)]
        hosts: Vec<String>,
    },

    /// Database table access
    #[serde(rename = "database")]
    Database {
        tables: Vec<String>,
        #[serde(default)]
        operations: Vec<DbOperation>,
    },

    /// External API access (general purpose)
    #[serde(rename = "external_api")]
    ExternalApi {
        endpoints: Vec<String>,
        #[serde(default)]
        rate_limit_per_minute: u32,
    },

    /// LLM/AI model access
    #[serde(rename = "llm")]
    Llm {
        providers: Vec<String>,
        #[serde(default)]
        max_tokens_per_request: u32,
    },

    /// Skill execution capability
    #[serde(rename = "skill")]
    Skill {
        skill_ids: Vec<String>,
        #[serde(default)]
        allow_install: bool,
    },

    /// Wallet/Blockchain transaction capability
    #[serde(rename = "wallet")]
    Wallet {
        chain_ids: Vec<u64>,
        #[serde(default)]
        max_transaction_value: Option<String>, // String for decimal precision
    },

    /// Agent spawning capability (for sub-agents)
    #[serde(rename = "spawn")]
    Spawn {
        max_concurrent: u32,
        #[serde(default)]
        allowed_capabilities: Vec<Box<AgentCapability>>,
    },
}

/// HTTP methods for network_http capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

/// Database operations for database capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbOperation {
    Select,
    Insert,
    Update,
    Delete,
}

impl Default for DbOperation {
    fn default() -> Self {
        DbOperation::Select
    }
}

impl AgentCapability {
    /// Convert AgentCapability to a compact string representation
    /// for backward compatibility with existing database storage
    pub fn to_compact_string(&self) -> String {
        match self {
            AgentCapability::FileRead { paths } => {
                format!("file:read:{}", paths.join(","))
            }
            AgentCapability::FileWrite { paths } => {
                format!("file:write:{}", paths.join(","))
            }
            AgentCapability::NetworkHttp { hosts, methods } => {
                let methods_str = if methods.is_empty() {
                    "*".to_string()
                } else {
                    methods
                        .iter()
                        .map(|m| format!("{:?}", m).to_uppercase())
                        .collect::<Vec<_>>()
                        .join(",")
                };
                format!("http:{}:{}", methods_str, hosts.join(","))
            }
            AgentCapability::NetworkTcp { ports, hosts } => {
                let ports_str = ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let hosts_str = if hosts.is_empty() {
                    "*".to_string()
                } else {
                    hosts.join(",")
                };
                format!("tcp:{}:{}", ports_str, hosts_str)
            }
            AgentCapability::Database { tables, operations } => {
                let ops_str = operations
                    .iter()
                    .map(|o| format!("{:?}", o).to_lowercase())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("db:{}:{}", ops_str, tables.join(","))
            }
            AgentCapability::ExternalApi {
                endpoints,
                rate_limit_per_minute,
            } => {
                format!("api:{}:{}", rate_limit_per_minute, endpoints.join(","))
            }
            AgentCapability::Llm {
                providers,
                max_tokens_per_request,
            } => {
                format!("llm:{}:{}", max_tokens_per_request, providers.join(","))
            }
            AgentCapability::Skill {
                skill_ids,
                allow_install,
            } => {
                format!("skill:{}:{}", allow_install, skill_ids.join(","))
            }
            AgentCapability::Wallet {
                chain_ids,
                max_transaction_value,
            } => {
                let max_val = max_transaction_value.as_deref().unwrap_or("unlimited");
                let chains = chain_ids
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("wallet:{}:{}", max_val, chains)
            }
            AgentCapability::Spawn {
                max_concurrent,
                allowed_capabilities,
            } => {
                let caps_str = allowed_capabilities
                    .iter()
                    .map(|c| c.to_compact_string())
                    .collect::<Vec<_>>()
                    .join(";");
                format!("spawn:{}:{}", max_concurrent, caps_str)
            }
        }
    }

    /// Parse from compact string representation (backward compatibility)
    pub fn from_compact_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() < 2 {
            return None;
        }

        match parts[0] {
            "file" => {
                let paths = parts.get(2)?.split(',').map(|s| s.to_string()).collect();
                match parts[1] {
                    "read" => Some(AgentCapability::FileRead { paths }),
                    "write" => Some(AgentCapability::FileWrite { paths }),
                    _ => None,
                }
            }
            "http" => {
                let hosts = parts.get(2)?.split(',').map(|s| s.to_string()).collect();
                let methods = if parts[1] == "*" {
                    vec![]
                } else {
                    parts[1]
                        .split(',')
                        .filter_map(|m| match m {
                            "GET" => Some(HttpMethod::Get),
                            "POST" => Some(HttpMethod::Post),
                            "PUT" => Some(HttpMethod::Put),
                            "DELETE" => Some(HttpMethod::Delete),
                            "PATCH" => Some(HttpMethod::Patch),
                            _ => None,
                        })
                        .collect()
                };
                Some(AgentCapability::NetworkHttp { hosts, methods })
            }
            "tcp" => {
                let ports = parts
                    .get(1)?
                    .split(',')
                    .filter_map(|p| p.parse::<u16>().ok())
                    .collect();
                let hosts = if let Some(h) = parts.get(2) {
                    if *h == "*" {
                        vec![]
                    } else {
                        h.split(',').map(|s| s.to_string()).collect()
                    }
                } else {
                    vec![]
                };
                Some(AgentCapability::NetworkTcp { ports, hosts })
            }
            "db" => {
                let operations = parts
                    .get(1)?
                    .split(',')
                    .filter_map(|o| match o.to_lowercase().as_str() {
                        "select" => Some(DbOperation::Select),
                        "insert" => Some(DbOperation::Insert),
                        "update" => Some(DbOperation::Update),
                        "delete" => Some(DbOperation::Delete),
                        _ => None,
                    })
                    .collect();
                let tables = parts.get(2)?.split(',').map(|s| s.to_string()).collect();
                Some(AgentCapability::Database { tables, operations })
            }
            "api" => {
                let rate_limit_per_minute = parts.get(1)?.parse::<u32>().unwrap_or(60);
                let endpoints = parts.get(2)?.split(',').map(|s| s.to_string()).collect();
                Some(AgentCapability::ExternalApi {
                    endpoints,
                    rate_limit_per_minute,
                })
            }
            "llm" => {
                let max_tokens_per_request = parts.get(1)?.parse::<u32>().unwrap_or(4096);
                let providers = parts.get(2)?.split(',').map(|s| s.to_string()).collect();
                Some(AgentCapability::Llm {
                    providers,
                    max_tokens_per_request,
                })
            }
            "skill" => {
                let allow_install = parts.get(1)?.parse::<bool>().unwrap_or(false);
                let skill_ids = parts.get(2)?.split(',').map(|s| s.to_string()).collect();
                Some(AgentCapability::Skill {
                    skill_ids,
                    allow_install,
                })
            }
            "wallet" => {
                let max_transaction_value = if let Some(m) = parts.get(1) {
                    if *m == "unlimited" {
                        None
                    } else {
                        Some(m.to_string())
                    }
                } else {
                    None
                };
                let chain_ids = parts
                    .get(2)?
                    .split(',')
                    .filter_map(|c| c.parse::<u64>().ok())
                    .collect();
                Some(AgentCapability::Wallet {
                    chain_ids,
                    max_transaction_value,
                })
            }
            "spawn" => {
                let max_concurrent = parts.get(1)?.parse::<u32>().unwrap_or(1);
                let allowed_capabilities = if let Some(caps) = parts.get(2) {
                    caps.split(';')
                        .filter_map(|c| AgentCapability::from_compact_string(c))
                        .map(Box::new)
                        .collect()
                } else {
                    vec![]
                };
                Some(AgentCapability::Spawn {
                    max_concurrent,
                    allowed_capabilities,
                })
            }
            _ => None, // Unknown type
        }
    }

    /// Convert to kernel CapabilitySet
    ///
    /// This method maps the high-level AgentCapability to the kernel's
    /// low-level capability system for sandboxed execution.
    pub fn to_kernel_capability_set(&self) -> beebotos_kernel::capabilities::CapabilitySet {
        use beebotos_kernel::capabilities::CapabilitySet;

        // Determine the capability level based on the capability type
        let level = self.suggested_capability_level();

        let mut kernel_caps = CapabilitySet::standard().with_level(level);

        // Add specific permission strings for application-level capabilities
        kernel_caps.permissions.insert(self.to_compact_string());

        kernel_caps
    }

    /// Get capability level based on the capability type
    pub fn suggested_capability_level(&self) -> beebotos_kernel::CapabilityLevel {
        use beebotos_kernel::CapabilityLevel;

        match self {
            AgentCapability::FileRead { .. } => CapabilityLevel::L1FileRead,
            AgentCapability::FileWrite { .. } => CapabilityLevel::L2FileWrite,
            AgentCapability::NetworkHttp { .. } | AgentCapability::NetworkTcp { .. } => {
                CapabilityLevel::L4NetworkIn
            }
            AgentCapability::Spawn { .. } => CapabilityLevel::L5SpawnLimited,
            _ => CapabilityLevel::L0LocalCompute,
        }
    }

    /// Get a human-readable description of this capability
    pub fn description(&self) -> String {
        match self {
            AgentCapability::FileRead { paths } => {
                format!("Read files in: {}", paths.join(", "))
            }
            AgentCapability::FileWrite { paths } => {
                format!("Write files in: {}", paths.join(", "))
            }
            AgentCapability::NetworkHttp { hosts, methods } => {
                let methods_str = if methods.is_empty() {
                    "all methods".to_string()
                } else {
                    methods
                        .iter()
                        .map(|m| format!("{:?}", m))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                format!("HTTP {} to: {}", methods_str, hosts.join(", "))
            }
            AgentCapability::NetworkTcp { ports, hosts } => {
                let hosts_str = if hosts.is_empty() {
                    "all hosts".to_string()
                } else {
                    hosts.join(", ")
                };
                format!(
                    "TCP ports {} on: {}",
                    ports
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    hosts_str
                )
            }
            AgentCapability::Database { tables, operations } => {
                let ops_str = operations
                    .iter()
                    .map(|o| format!("{:?}", o))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Database {} on tables: {}", ops_str, tables.join(", "))
            }
            AgentCapability::ExternalApi {
                endpoints,
                rate_limit_per_minute,
            } => {
                format!(
                    "API access to {} endpoints ({} req/min)",
                    endpoints.len(),
                    rate_limit_per_minute
                )
            }
            AgentCapability::Llm {
                providers,
                max_tokens_per_request,
            } => {
                format!(
                    "LLM access via {} (max {} tokens)",
                    providers.join(", "),
                    max_tokens_per_request
                )
            }
            AgentCapability::Skill {
                skill_ids,
                allow_install,
            } => {
                let install_str = if *allow_install {
                    " with install permission"
                } else {
                    ""
                };
                format!("Execute skills: {}{}", skill_ids.join(", "), install_str)
            }
            AgentCapability::Wallet {
                chain_ids,
                max_transaction_value,
            } => {
                let val_str = max_transaction_value.as_deref().unwrap_or("unlimited");
                format!("Wallet on chains {:?} (max {})", chain_ids, val_str)
            }
            AgentCapability::Spawn {
                max_concurrent,
                allowed_capabilities,
            } => {
                format!(
                    "Spawn up to {} sub-agents with {} capabilities",
                    max_concurrent,
                    allowed_capabilities.len()
                )
            }
        }
    }
}

/// Capability set for an agent with type-safe operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentCapabilitySet {
    pub capabilities: Vec<AgentCapability>,
}

impl AgentCapabilitySet {
    /// Create empty capability set
    pub fn new() -> Self {
        Self {
            capabilities: Vec::new(),
        }
    }

    /// Create with standard safe defaults
    pub fn standard() -> Self {
        Self {
            capabilities: vec![
                AgentCapability::FileRead {
                    paths: vec!["/tmp".to_string()],
                },
                AgentCapability::NetworkHttp {
                    hosts: vec!["api.openai.com".to_string()],
                    methods: vec![HttpMethod::Post],
                },
            ],
        }
    }

    /// Add a capability
    pub fn with_capability(mut self, cap: AgentCapability) -> Self {
        self.capabilities.push(cap);
        self
    }

    /// Convert to kernel CapabilitySet
    pub fn to_kernel_capability_set(&self) -> beebotos_kernel::capabilities::CapabilitySet {
        use beebotos_kernel::capabilities::CapabilitySet;
        use beebotos_kernel::CapabilityLevel;

        // Determine the highest required capability level
        let max_level = self
            .capabilities
            .iter()
            .map(|c| c.suggested_capability_level())
            .max()
            .unwrap_or(CapabilityLevel::L0LocalCompute);

        let mut kernel_caps = CapabilitySet::standard().with_level(max_level);

        // Add specific capabilities
        for cap in &self.capabilities {
            // Add permission string for application-level capabilities
            kernel_caps.permissions.insert(cap.to_compact_string());
        }

        kernel_caps
    }

    /// Convert to JSON string for database storage
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.capabilities)
    }

    /// Parse from JSON string
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        let capabilities: Vec<AgentCapability> = serde_json::from_str(s)?;
        Ok(Self { capabilities })
    }

    /// Convert to legacy string format for backward compatibility
    pub fn to_legacy_strings(&self) -> Vec<String> {
        self.capabilities
            .iter()
            .map(|c| c.to_compact_string())
            .collect()
    }

    /// Check if has a specific capability type
    pub fn has_capability(&self, cap_type: &str) -> bool {
        self.capabilities.iter().any(|c| {
            matches!(
                (c, cap_type),
                (AgentCapability::FileRead { .. }, "file_read")
                    | (AgentCapability::FileWrite { .. }, "file_write")
                    | (AgentCapability::NetworkHttp { .. }, "network_http")
                    | (AgentCapability::NetworkTcp { .. }, "network_tcp")
                    | (AgentCapability::Database { .. }, "database")
                    | (AgentCapability::ExternalApi { .. }, "external_api")
                    | (AgentCapability::Llm { .. }, "llm")
                    | (AgentCapability::Skill { .. }, "skill")
                    | (AgentCapability::Wallet { .. }, "wallet")
                    | (AgentCapability::Spawn { .. }, "spawn")
            )
        })
    }

    /// Get all file read paths
    pub fn file_read_paths(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .filter_map(|c| match c {
                AgentCapability::FileRead { paths } => Some(paths.iter().map(|s| s.as_str())),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Get all HTTP hosts
    pub fn http_hosts(&self) -> Vec<&str> {
        self.capabilities
            .iter()
            .filter_map(|c| match c {
                AgentCapability::NetworkHttp { hosts, .. } => {
                    Some(hosts.iter().map(|s| s.as_str()))
                }
                _ => None,
            })
            .flatten()
            .collect()
    }
}

impl From<Vec<String>> for AgentCapabilitySet {
    fn from(strings: Vec<String>) -> Self {
        let capabilities = strings
            .into_iter()
            .filter_map(|s| {
                // Try JSON first
                if let Ok(cap) = serde_json::from_str::<AgentCapability>(&s) {
                    Some(cap)
                } else {
                    // Fall back to compact string
                    AgentCapability::from_compact_string(&s)
                }
            })
            .collect();
        Self { capabilities }
    }
}

impl From<AgentCapabilitySet> for Vec<String> {
    fn from(set: AgentCapabilitySet) -> Self {
        set.to_legacy_strings()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_serialization() {
        let cap = AgentCapability::FileRead {
            paths: vec!["/tmp".to_string(), "/data".to_string()],
        };
        let json = serde_json::to_string(&cap).unwrap();
        assert!(json.contains("file_read"));
        assert!(json.contains("/tmp"));

        let deserialized: AgentCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, deserialized);
    }

    #[test]
    fn test_capability_set_to_kernel() {
        let set = AgentCapabilitySet::standard().with_capability(AgentCapability::FileWrite {
            paths: vec!["/output".to_string()],
        });

        let kernel_set = set.to_kernel_capability_set();
        assert!(!kernel_set.permissions.is_empty());
    }

    #[test]
    fn test_compact_string_roundtrip() {
        let cap = AgentCapability::FileRead {
            paths: vec!["/tmp".to_string(), "/var".to_string()],
        };
        let compact = cap.to_compact_string();
        let parsed = AgentCapability::from_compact_string(&compact);
        assert_eq!(Some(cap), parsed);
    }
}
