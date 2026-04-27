//! Resource Allocation and Management

use std::collections::{HashMap, HashSet};

use tokio::sync::Mutex;

use super::TaskId;

/// Resource types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// CPU cores
    Cpu,
    /// Memory (bytes)
    Memory,
    /// GPU
    Gpu,
    /// TEE enclave
    Tee,
    /// Storage I/O
    StorageIo,
    /// Network bandwidth
    Network,
    /// File descriptor
    FileDescriptor,
    /// Agent-specific resource
    Agent(String),
}

/// Resource units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceUnit {
    /// Count-based unit
    Count(u64),
    /// Byte-based unit
    Bytes(u64),
    /// Percentage-based unit (0-100)
    Percent(u8),
    /// Weighted shares unit
    Shares(u64),
}

impl ResourceUnit {
    /// Get as count
    pub fn as_count(&self) -> u64 {
        match self {
            Self::Count(n) => *n,
            _ => 0,
        }
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> u64 {
        match self {
            Self::Bytes(n) => *n,
            _ => 0,
        }
    }
}

/// Resource request
#[derive(Debug, Clone)]
pub struct ResourceRequest {
    /// Resource type
    pub resource_type: ResourceType,
    /// Requested amount
    pub amount: ResourceUnit,
    /// Request priority
    pub priority: u8,
    /// Deadline in milliseconds
    pub deadline_ms: Option<u64>,
}

/// Resource allocation
#[derive(Debug, Clone)]
pub struct ResourceAllocation {
    /// Task ID
    pub task_id: TaskId,
    /// Resource type
    pub resource_type: ResourceType,
    /// Allocated amount
    pub amount: ResourceUnit,
    /// Allocation timestamp
    pub allocated_at: u64,
    /// Expiration timestamp
    pub expires_at: Option<u64>,
}

/// Resource allocator
pub struct ResourceAllocator {
    /// Total available resources
    total: Mutex<HashMap<ResourceType, ResourceUnit>>,
    /// Currently allocated
    allocated: Mutex<HashMap<TaskId, Vec<ResourceAllocation>>>,
    /// Wait queue for resources
    wait_queue: Mutex<Vec<(TaskId, ResourceRequest)>>,
    /// Resource reservations
    reservations: Mutex<HashMap<String, HashSet<TaskId>>>,
}

impl ResourceAllocator {
    /// Create new resource allocator
    pub fn new() -> Self {
        let mut total = HashMap::new();
        total.insert(
            ResourceType::Cpu,
            ResourceUnit::Count(num_cpus::get() as u64),
        );
        total.insert(
            ResourceType::Memory,
            ResourceUnit::Bytes(sysinfo::System::new_all().total_memory()),
        );

        Self {
            total: Mutex::new(total),
            allocated: Mutex::new(HashMap::new()),
            wait_queue: Mutex::new(Vec::new()),
            reservations: Mutex::new(HashMap::new()),
        }
    }

    /// Request resource allocation
    pub async fn request(
        &self,
        task_id: TaskId,
        request: ResourceRequest,
    ) -> Result<ResourceAllocation, ResourceError> {
        // Check if available
        if self.is_available(&request).await {
            self.allocate(task_id, request).await
        } else {
            // Add to wait queue
            self.wait_queue.lock().await.push((task_id, request));
            Err(ResourceError::Unavailable)
        }
    }

    /// Check if resource is available
    async fn is_available(&self, request: &ResourceRequest) -> bool {
        let total = self.total.lock().await;
        let allocated = self.allocated.lock().await;

        let total_amount = match total.get(&request.resource_type) {
            Some(a) => a,
            None => return false,
        };

        let used: ResourceUnit = allocated
            .values()
            .flatten()
            .filter(|a| a.resource_type == request.resource_type)
            .map(|a| a.amount)
            .fold(ResourceUnit::Count(0), |acc, a| acc.add(a));

        total_amount.can_accommodate(&used, &request.amount)
    }

    /// Allocate resource
    async fn allocate(
        &self,
        task_id: TaskId,
        request: ResourceRequest,
    ) -> Result<ResourceAllocation, ResourceError> {
        let allocation = ResourceAllocation {
            task_id,
            resource_type: request.resource_type.clone(),
            amount: request.amount,
            allocated_at: now(),
            expires_at: request.deadline_ms.map(|d| now() + d),
        };

        let mut allocated = self.allocated.lock().await;
        allocated
            .entry(task_id)
            .or_default()
            .push(allocation.clone());

        Ok(allocation)
    }

    /// Release resource
    pub async fn release(
        &self,
        task_id: TaskId,
        resource_type: Option<&ResourceType>,
    ) -> Vec<ResourceAllocation> {
        let mut allocated = self.allocated.lock().await;

        if let Some(allocs) = allocated.get_mut(&task_id) {
            if let Some(rtype) = resource_type {
                let mut released = Vec::new();
                allocs.retain(|a| {
                    if &a.resource_type == rtype {
                        released.push(a.clone());
                        false
                    } else {
                        true
                    }
                });

                if allocs.is_empty() {
                    allocated.remove(&task_id);
                }

                // Try to allocate waiting tasks
                drop(allocated);
                self.process_wait_queue().await;

                return released;
            } else {
                return allocated.remove(&task_id).unwrap_or_default();
            }
        }

        Vec::new()
    }

    /// Process wait queue
    async fn process_wait_queue(&self) {
        let mut wait_queue = self.wait_queue.lock().await;
        let mut to_remove = Vec::new();

        for (i, (task_id, request)) in wait_queue.iter().enumerate() {
            if self.is_available(request).await {
                if let Ok(_allocation) = self.allocate(*task_id, request.clone()).await {
                    to_remove.push(i);
                    // Signal task that resource is available
                    // (would need channel mechanism)
                }
            }
        }

        // Remove allocated requests (in reverse order)
        for i in to_remove.into_iter().rev() {
            wait_queue.remove(i);
        }
    }

    /// Get allocations for task
    pub async fn get_allocations(&self, task_id: TaskId) -> Vec<ResourceAllocation> {
        self.allocated
            .lock()
            .await
            .get(&task_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Reserve resources for specific use
    pub async fn reserve(&self, reservation_id: impl Into<String>, task_ids: Vec<TaskId>) {
        let mut reservations = self.reservations.lock().await;
        let set: HashSet<_> = task_ids.into_iter().collect();
        reservations.insert(reservation_id.into(), set);
    }

    /// Check if task has reservation
    pub async fn has_reservation(&self, reservation_id: &str, task_id: TaskId) -> bool {
        self.reservations
            .lock()
            .await
            .get(reservation_id)
            .is_some_and(|set| set.contains(&task_id))
    }

    /// Get resource usage statistics
    pub async fn usage_stats(&self) -> ResourceUsageStats {
        let total = self.total.lock().await;
        let allocated = self.allocated.lock().await;

        let mut stats = ResourceUsageStats::default();

        for (rtype, total_amount) in total.iter() {
            let used: ResourceUnit = allocated
                .values()
                .flatten()
                .filter(|a| &a.resource_type == rtype)
                .map(|a| a.amount)
                .fold(ResourceUnit::Count(0), |acc, a| acc.add(a));

            let utilization = total_amount.utilization(&used);

            stats.by_type.insert(
                rtype.clone(),
                ResourceStat {
                    total: *total_amount,
                    used,
                    utilization,
                },
            );
        }

        stats.total_allocations = allocated.len() as u64;
        stats.waiting_requests = self.wait_queue.lock().await.len() as u64;

        stats
    }
}

impl Default for ResourceAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource allocation error
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceError {
    /// Resource is unavailable
    Unavailable,
    /// Insufficient capacity to fulfill request
    InsufficientCapacity,
    /// Not authorized to access resource
    NotAuthorized,
    /// Invalid resource request
    InvalidRequest,
    /// Resource allocation expired
    Expired,
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "Resource unavailable"),
            Self::InsufficientCapacity => write!(f, "Insufficient capacity"),
            Self::NotAuthorized => write!(f, "Not authorized"),
            Self::InvalidRequest => write!(f, "Invalid request"),
            Self::Expired => write!(f, "Allocation expired"),
        }
    }
}

impl std::error::Error for ResourceError {}

/// Resource usage statistics
#[derive(Debug, Clone, Default)]
pub struct ResourceUsageStats {
    /// Stats by resource type
    pub by_type: HashMap<ResourceType, ResourceStat>,
    /// Total allocations
    pub total_allocations: u64,
    /// Waiting requests
    pub waiting_requests: u64,
}

/// Resource statistic
#[derive(Debug, Clone)]
pub struct ResourceStat {
    /// Total capacity
    pub total: ResourceUnit,
    /// Used capacity
    pub used: ResourceUnit,
    /// Utilization ratio
    pub utilization: f64,
}

/// ResourceUnit arithmetic
impl ResourceUnit {
    /// Add two resource units
    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Count(a), Self::Count(b)) => Self::Count(a + b),
            (Self::Bytes(a), Self::Bytes(b)) => Self::Bytes(a + b),
            (Self::Percent(a), Self::Percent(b)) => Self::Percent((a + b).min(100)),
            (Self::Shares(a), Self::Shares(b)) => Self::Shares(a + b),
            _ => self, // Incompatible types
        }
    }

    /// Check if can accommodate
    fn can_accommodate(&self, used: &Self, request: &Self) -> bool {
        match (self, used, request) {
            (Self::Count(total), Self::Count(u), Self::Count(r)) => u + r <= *total,
            (Self::Bytes(total), Self::Bytes(u), Self::Bytes(r)) => u + r <= *total,
            (Self::Percent(total), Self::Percent(u), Self::Percent(r)) => u + r <= *total,
            _ => false,
        }
    }

    /// Calculate utilization
    fn utilization(&self, used: &Self) -> f64 {
        match (self, used) {
            (Self::Count(t), Self::Count(u)) => *u as f64 / *t as f64,
            (Self::Bytes(t), Self::Bytes(u)) => *u as f64 / *t as f64,
            (Self::Percent(t), Self::Percent(u)) => *u as f64 / *t as f64,
            _ => 0.0,
        }
    }
}

/// CPU affinity for tasks
pub struct CpuAffinity {
    /// Allowed CPUs
    allowed_cpus: Vec<usize>,
    /// Current CPU
    current_cpu: Mutex<Option<usize>>,
}

impl CpuAffinity {
    /// Create new CPU affinity
    pub fn new(allowed: Vec<usize>) -> Self {
        Self {
            allowed_cpus: allowed,
            current_cpu: Mutex::new(None),
        }
    }

    /// Bind to specific CPU
    pub async fn bind(&self, cpu: usize) -> Result<(), ResourceError> {
        if !self.allowed_cpus.contains(&cpu) {
            return Err(ResourceError::InvalidRequest);
        }

        *self.current_cpu.lock().await = Some(cpu);
        Ok(())
    }

    /// Get current CPU
    pub async fn current(&self) -> Option<usize> {
        *self.current_cpu.lock().await
    }

    /// Get allowed CPUs
    pub fn allowed(&self) -> &[usize] {
        &self.allowed_cpus
    }

    /// Create from bitmask
    pub fn from_bitmask(mask: u64) -> Self {
        let mut allowed = Vec::new();
        for i in 0..64 {
            if mask & (1 << i) != 0 {
                allowed.push(i);
            }
        }
        Self::new(allowed)
    }

    /// Convert to bitmask
    pub fn to_bitmask(&self) -> u64 {
        self.allowed_cpus
            .iter()
            .fold(0u64, |acc, &cpu| acc | (1 << cpu))
    }
}

/// Memory allocator for tasks
pub struct TaskMemoryAllocator {
    /// Maximum memory per task
    max_per_task: usize,
    /// Total allocated
    total_allocated: Mutex<usize>,
    /// Per-task allocations
    allocations: Mutex<HashMap<TaskId, usize>>,
}

impl TaskMemoryAllocator {
    /// Create new memory allocator
    pub fn new(max_per_task: usize) -> Self {
        Self {
            max_per_task,
            total_allocated: Mutex::new(0),
            allocations: Mutex::new(HashMap::new()),
        }
    }

    /// Allocate memory
    pub async fn allocate(&self, task_id: TaskId, size: usize) -> Result<usize, ResourceError> {
        if size > self.max_per_task {
            return Err(ResourceError::InsufficientCapacity);
        }

        let mut allocations = self.allocations.lock().await;
        let current = allocations.get(&task_id).copied().unwrap_or(0);

        if current + size > self.max_per_task {
            return Err(ResourceError::InsufficientCapacity);
        }

        allocations.insert(task_id, current + size);
        *self.total_allocated.lock().await += size;

        Ok(size)
    }

    /// Free memory
    pub async fn free(&self, task_id: TaskId, size: usize) {
        let mut allocations = self.allocations.lock().await;

        if let Some(current) = allocations.get_mut(&task_id) {
            *current = current.saturating_sub(size);
            if *current == 0 {
                allocations.remove(&task_id);
            }
        }

        *self.total_allocated.lock().await -= size;
    }

    /// Get task memory usage
    pub async fn usage(&self, task_id: TaskId) -> usize {
        self.allocations
            .lock()
            .await
            .get(&task_id)
            .copied()
            .unwrap_or(0)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
