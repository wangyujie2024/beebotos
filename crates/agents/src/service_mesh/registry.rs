//! Service Registry Module
//!
//! 服务注册表接口和实现，支持内存和分布式存储后端。

use std::collections::HashMap;
use std::net::SocketAddr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 服务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    /// 注册中
    Registering,
    /// 健康
    Healthy,
    /// 不健康
    Unhealthy,
    /// 维护中
    Maintenance,
    /// 已注销
    Deregistered,
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceState::Registering => write!(f, "registering"),
            ServiceState::Healthy => write!(f, "healthy"),
            ServiceState::Unhealthy => write!(f, "unhealthy"),
            ServiceState::Maintenance => write!(f, "maintenance"),
            ServiceState::Deregistered => write!(f, "deregistered"),
        }
    }
}

/// 服务端点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    /// 协议类型
    pub protocol: Protocol,
    /// 地址
    pub address: SocketAddr,
    /// 路径（可选）
    pub path: Option<String>,
    /// 权重（用于负载均衡）
    pub weight: u32,
}

/// 协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Http,
    Https,
    Grpc,
    WebSocket,
    WebSocketSecure,
    Tcp,
    Udp,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Http => write!(f, "http"),
            Protocol::Https => write!(f, "https"),
            Protocol::Grpc => write!(f, "grpc"),
            Protocol::WebSocket => write!(f, "ws"),
            Protocol::WebSocketSecure => write!(f, "wss"),
            Protocol::Tcp => write!(f, "tcp"),
            Protocol::Udp => write!(f, "udp"),
        }
    }
}

/// 服务条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// 服务实例唯一 ID
    pub id: String,
    /// 服务名称
    pub name: String,
    /// 服务版本
    pub version: String,
    /// 服务端点列表
    pub endpoints: Vec<ServiceEndpoint>,
    /// 服务状态
    pub state: ServiceState,
    /// 关联的 DID（可选）
    pub did: Option<String>,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 标签（用于服务发现）
    pub tags: Vec<String>,
    /// 注册时间戳
    pub registered_at: u64,
    /// 最后心跳时间戳
    pub last_heartbeat_at: u64,
    /// TTL（秒）
    pub ttl_secs: u64,
}

impl ServiceEntry {
    /// 创建新的服务条目
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            endpoints: vec![],
            state: ServiceState::Registering,
            did: None,
            metadata: HashMap::new(),
            tags: vec![],
            registered_at: now,
            last_heartbeat_at: now,
            ttl_secs: 60,
        }
    }

    /// 添加端点
    pub fn with_endpoint(mut self, endpoint: ServiceEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    /// 设置 DID
    pub fn with_did(mut self, did: impl Into<String>) -> Self {
        self.did = Some(did.into());
        self
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// 添加标签
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// 设置 TTL
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }

    /// 检查服务是否过期
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.last_heartbeat_at + self.ttl_secs
    }

    /// 更新心跳
    pub fn heartbeat(&mut self) {
        self.last_heartbeat_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// 获取主端点 URL
    pub fn primary_url(&self) -> Option<String> {
        self.endpoints.first().map(|ep| {
            let base = format!("{}://{}", ep.protocol, ep.address);
            match &ep.path {
                Some(path) => format!("{}{}", base, path),
                None => base,
            }
        })
    }
}

/// 服务注册表错误
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Service not found: {0}")]
    NotFound(String),
    #[error("Service already exists: {0}")]
    AlreadyExists(String),
    #[error("Invalid service entry: {0}")]
    InvalidEntry(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Lock error: {0}")]
    Lock(String),
}

/// 服务注册表 trait
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// 注册服务
    async fn register(&self, entry: ServiceEntry) -> Result<(), RegistryError>;

    /// 注销服务
    async fn deregister(&self, service_id: &str) -> Result<(), RegistryError>;

    /// 获取服务
    async fn get(&self, service_id: &str) -> Result<Option<ServiceEntry>, RegistryError>;

    /// 按名称查找服务
    async fn find_by_name(&self, name: &str) -> Result<Vec<ServiceEntry>, RegistryError>;

    /// 按标签查找服务
    async fn find_by_tag(&self, tag: &str) -> Result<Vec<ServiceEntry>, RegistryError>;

    /// 按 DID 查找服务
    async fn find_by_did(&self, did: &str) -> Result<Option<ServiceEntry>, RegistryError>;

    /// 列出所有服务
    async fn list_all(&self) -> Result<Vec<ServiceEntry>, RegistryError>;

    /// 更新服务状态
    async fn update_state(
        &self,
        service_id: &str,
        state: ServiceState,
    ) -> Result<(), RegistryError>;

    /// 更新心跳
    async fn heartbeat(&self, service_id: &str) -> Result<(), RegistryError>;

    /// 清理过期服务
    async fn cleanup_expired(&self) -> Result<u32, RegistryError>;
}

/// 内存服务注册表实现
pub struct InMemoryServiceRegistry {
    services: RwLock<HashMap<String, ServiceEntry>>,
    name_index: RwLock<HashMap<String, Vec<String>>>,
    tag_index: RwLock<HashMap<String, Vec<String>>>,
    did_index: RwLock<HashMap<String, String>>,
}

impl InMemoryServiceRegistry {
    /// 创建新的内存注册表
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            tag_index: RwLock::new(HashMap::new()),
            did_index: RwLock::new(HashMap::new()),
        }
    }

    /// 构建索引
    async fn build_indices(&self, entry: &ServiceEntry) {
        // 名称索引
        let mut name_index = self.name_index.write().await;
        name_index
            .entry(entry.name.clone())
            .or_default()
            .push(entry.id.clone());

        // 标签索引
        let mut tag_index = self.tag_index.write().await;
        for tag in &entry.tags {
            tag_index
                .entry(tag.clone())
                .or_default()
                .push(entry.id.clone());
        }

        // DID 索引
        if let Some(ref did) = entry.did {
            let mut did_index = self.did_index.write().await;
            did_index.insert(did.clone(), entry.id.clone());
        }
    }

    /// 移除索引
    async fn remove_indices(&self, entry: &ServiceEntry) {
        // 名称索引
        let mut name_index = self.name_index.write().await;
        if let Some(ids) = name_index.get_mut(&entry.name) {
            ids.retain(|id| id != &entry.id);
        }

        // 标签索引
        let mut tag_index = self.tag_index.write().await;
        for tag in &entry.tags {
            if let Some(ids) = tag_index.get_mut(tag) {
                ids.retain(|id| id != &entry.id);
            }
        }

        // DID 索引
        if let Some(ref did) = entry.did {
            let mut did_index = self.did_index.write().await;
            did_index.remove(did);
        }
    }
}

#[async_trait]
impl ServiceRegistry for InMemoryServiceRegistry {
    async fn register(&self, entry: ServiceEntry) -> Result<(), RegistryError> {
        debug!("Registering service: {}", entry.id);

        let services = self.services.read().await;

        if services.contains_key(&entry.id) {
            return Err(RegistryError::AlreadyExists(entry.id.clone()));
        }

        let entry_id = entry.id.clone();

        // 构建索引
        drop(services);
        self.build_indices(&entry).await;

        // 存储服务
        let mut services = self.services.write().await;
        services.insert(entry_id.clone(), entry);

        info!("Service registered: {}", entry_id);
        Ok(())
    }

    async fn deregister(&self, service_id: &str) -> Result<(), RegistryError> {
        debug!("Deregistering service: {}", service_id);

        let mut services = self.services.write().await;

        let entry = services
            .get(service_id)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(service_id.to_string()))?;

        services.remove(service_id);
        drop(services);

        // 移除索引
        self.remove_indices(&entry).await;

        info!("Service deregistered: {}", service_id);
        Ok(())
    }

    async fn get(&self, service_id: &str) -> Result<Option<ServiceEntry>, RegistryError> {
        let services = self.services.read().await;
        Ok(services.get(service_id).cloned())
    }

    async fn find_by_name(&self, name: &str) -> Result<Vec<ServiceEntry>, RegistryError> {
        let name_index = self.name_index.read().await;
        let service_ids = name_index.get(name).cloned().unwrap_or_default();
        drop(name_index);

        let services = self.services.read().await;
        let entries: Vec<_> = service_ids
            .iter()
            .filter_map(|id| services.get(id).cloned())
            .collect();

        Ok(entries)
    }

    async fn find_by_tag(&self, tag: &str) -> Result<Vec<ServiceEntry>, RegistryError> {
        let tag_index = self.tag_index.read().await;
        let service_ids = tag_index.get(tag).cloned().unwrap_or_default();
        drop(tag_index);

        let services = self.services.read().await;
        let entries: Vec<_> = service_ids
            .iter()
            .filter_map(|id| services.get(id).cloned())
            .collect();

        Ok(entries)
    }

    async fn find_by_did(&self, did: &str) -> Result<Option<ServiceEntry>, RegistryError> {
        let did_index = self.did_index.read().await;
        let service_id = did_index.get(did).cloned();
        drop(did_index);

        if let Some(id) = service_id {
            self.get(&id).await
        } else {
            Ok(None)
        }
    }

    async fn list_all(&self) -> Result<Vec<ServiceEntry>, RegistryError> {
        let services = self.services.read().await;
        Ok(services.values().cloned().collect())
    }

    async fn update_state(
        &self,
        service_id: &str,
        state: ServiceState,
    ) -> Result<(), RegistryError> {
        let mut services = self.services.write().await;

        let entry = services
            .get_mut(service_id)
            .ok_or_else(|| RegistryError::NotFound(service_id.to_string()))?;

        entry.state = state;
        debug!("Service {} state updated to {}", service_id, state);

        Ok(())
    }

    async fn heartbeat(&self, service_id: &str) -> Result<(), RegistryError> {
        let mut services = self.services.write().await;

        let entry = services
            .get_mut(service_id)
            .ok_or_else(|| RegistryError::NotFound(service_id.to_string()))?;

        entry.heartbeat();

        // 如果之前是不健康的，现在恢复为健康
        if entry.state == ServiceState::Unhealthy {
            entry.state = ServiceState::Healthy;
        }

        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u32, RegistryError> {
        let services = self.services.read().await;
        let expired_ids: Vec<_> = services
            .values()
            .filter(|entry| entry.is_expired())
            .map(|entry| entry.id.clone())
            .collect();
        drop(services);

        let mut count = 0;
        for id in expired_ids {
            if let Err(e) = self.deregister(&id).await {
                warn!("Failed to cleanup expired service {}: {}", id, e);
            } else {
                count += 1;
            }
        }

        if count > 0 {
            info!("Cleaned up {} expired services", count);
        }

        Ok(count)
    }
}

impl Default for InMemoryServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    #[test]
    fn test_service_state_display() {
        assert_eq!(ServiceState::Healthy.to_string(), "healthy");
        assert_eq!(ServiceState::Unhealthy.to_string(), "unhealthy");
    }

    #[tokio::test]
    async fn test_in_memory_registry() {
        let registry = InMemoryServiceRegistry::new();

        // 注册服务
        let entry = ServiceEntry::new("svc-1", "test-service", "1.0.0")
            .with_endpoint(ServiceEndpoint {
                protocol: Protocol::Http,
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                path: Some("/api".to_string()),
                weight: 1,
            })
            .with_tag("test");

        registry.register(entry.clone()).await.unwrap();

        // 查找服务
        let found = registry.get("svc-1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test-service");

        // 按名称查找
        let by_name = registry.find_by_name("test-service").await.unwrap();
        assert_eq!(by_name.len(), 1);

        // 按标签查找
        let by_tag = registry.find_by_tag("test").await.unwrap();
        assert_eq!(by_tag.len(), 1);

        // 注销服务
        registry.deregister("svc-1").await.unwrap();
        let found = registry.get("svc-1").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_service_expiry() {
        // Create entry with very short TTL (1 second)
        let entry = ServiceEntry::new("svc-1", "test-service", "1.0.0").with_ttl(1);

        // Wait for entry to expire
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        assert!(entry.is_expired());
    }

    #[tokio::test]
    async fn test_service_heartbeat() {
        let mut entry = ServiceEntry::new("svc-1", "test-service", "1.0.0").with_ttl(3600);

        let old_heartbeat = entry.last_heartbeat_at;

        // Small delay to ensure timestamp changes
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        entry.heartbeat();

        assert!(entry.last_heartbeat_at >= old_heartbeat);
        assert!(!entry.is_expired());
    }
}
