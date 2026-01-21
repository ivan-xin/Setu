# Setu Runtime 集成指南

## 概述

本文档说明如何在 Setu 的 Solver 中集成使用 `setu-runtime` 来执行交易。

## 在 Solver 中集成

### 1. 添加依赖

修改 `setu-solver/Cargo.toml`：

```toml
[dependencies]
setu-runtime = { path = "../crates/setu-runtime" }
setu-types = { path = "../types" }
```

### 2. 更新 Executor

修改 `setu-solver/src/executor.rs`：

```rust
use setu_runtime::{
    RuntimeExecutor, ExecutionContext, Transaction as RuntimeTransaction,
    InMemoryStateStore, StateStore, ExecutionOutput,
};
use setu_types::{ObjectId, Address};
use core_types::Transfer;

pub struct Executor {
    node_id: String,
    // 添加 runtime 执行器
    runtime: RuntimeExecutor<InMemoryStateStore>,
}

impl Executor {
    pub fn new(node_id: String) -> Self {
        let state = InMemoryStateStore::new();
        let runtime = RuntimeExecutor::new(state);
        
        Self {
            node_id,
            runtime,
        }
    }
    
    /// 从 core_types::Transfer 转换为 RuntimeTransaction
    fn convert_transfer_to_tx(
        &self,
        transfer: &Transfer,
    ) -> anyhow::Result<RuntimeTransaction> {
        // 解析地址和对象 ID
        let sender = Address::from(&transfer.from as &str);
        let recipient = Address::from(&transfer.to as &str);
        
        // 假设 transfer.id 包含了 coin_id 信息
        // 实际实现时需要从依赖对象中获取
        let coin_id = ObjectId::random(); // TODO: 从依赖对象获取
        
        Ok(RuntimeTransaction::new_transfer(
            sender,
            coin_id,
            recipient,
            Some(transfer.amount as u64),
        ))
    }
    
    pub async fn execute_in_tee(
        &mut self,
        transfer: &Transfer,
    ) -> anyhow::Result<ExecutionOutput> {
        info!(
            node_id = %self.node_id,
            transfer_id = %transfer.id,
            "Executing transfer in Runtime"
        );
        
        // 1. 转换为 RuntimeTransaction
        let tx = self.convert_transfer_to_tx(transfer)?;
        
        // 2. 创建执行上下文
        let ctx = ExecutionContext {
            executor_id: self.node_id.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            in_tee: false, // TODO: 未来在 TEE 中执行
        };
        
        // 3. 执行交易
        let output = self.runtime.execute_transaction(&tx, &ctx)?;
        
        info!(
            transfer_id = %transfer.id,
            success = output.success,
            changes = output.state_changes.len(),
            "Transfer executed"
        );
        
        Ok(output)
    }
    
    /// 获取状态存储的引用（用于查询）
    pub fn state(&self) -> &InMemoryStateStore {
        self.runtime.state()
    }
}
```

### 3. 处理依赖对象

在 `setu-solver/src/dependency.rs` 中，需要从依赖对象中提取实际的对象信息：

```rust
use setu_types::{Object, CoinData, ObjectId};
use setu_runtime::StateStore;

pub struct DependencyResolver {
    // 可以访问 runtime 的状态存储
}

impl DependencyResolver {
    /// 解析交易的依赖对象
    /// 返回该交易需要访问的所有对象 ID
    pub async fn resolve_dependencies(
        &self,
        transfer: &Transfer,
        state: &impl StateStore,
    ) -> anyhow::Result<Vec<ObjectId>> {
        // 从 sender 地址查询其拥有的 Coin 对象
        let sender = Address::from(&transfer.from as &str);
        let owned_objects = state.get_owned_objects(&sender)?;
        
        // 找到余额足够的 Coin
        for obj_id in owned_objects {
            if let Some(coin) = state.get_object(&obj_id)? {
                if coin.data.balance.value() >= transfer.amount as u64 {
                    return Ok(vec![obj_id]);
                }
            }
        }
        
        anyhow::bail!("No sufficient balance for transfer")
    }
}
```

### 4. 构建 Event

执行完成后，使用 `ExecutionOutput` 构建 Event：

```rust
use setu_types::{Event, EventType, ExecutionResult, StateChange as EventStateChange};
use setu_runtime::ExecutionOutput;

pub fn build_event_from_output(
    transfer: &Transfer,
    output: ExecutionOutput,
    vlc_snapshot: VLCSnapshot,
    creator: String,
) -> Event {
    // 将 RuntimeStateChange 转换为 EventStateChange
    let state_changes: Vec<EventStateChange> = output
        .state_changes
        .into_iter()
        .map(|change| EventStateChange {
            key: format!("object:{}", change.object_id),
            old_value: change.old_state,
            new_value: change.new_state,
        })
        .collect();
    
    let execution_result = ExecutionResult {
        success: output.success,
        message: output.message,
        state_changes,
    };
    
    let mut event = Event::new(
        EventType::Transfer,
        vec![], // parent_ids 从 DAG 获取
        vlc_snapshot,
        creator,
    );
    
    event.transfer = Some(transfer.clone());
    event.execution_result = Some(execution_result);
    
    event
}
```

## 完整流程示例

```rust
// 在 Solver 的主循环中
pub async fn process_transfer(
    &mut self,
    transfer: Transfer,
) -> anyhow::Result<Event> {
    // 1. 解析依赖对象
    let dependencies = self.dependency_resolver
        .resolve_dependencies(&transfer, self.executor.state())
        .await?;
    
    info!("Resolved {} dependencies", dependencies.len());
    
    // 2. 在 Runtime 中执行
    let output = self.executor.execute_in_tee(&transfer).await?;
    
    if !output.success {
        anyhow::bail!("Transfer execution failed: {:?}", output.message);
    }
    
    // 3. 构建 Event
    let vlc_snapshot = self.vlc.snapshot();
    let event = build_event_from_output(
        &transfer,
        output,
        vlc_snapshot,
        self.node_id.clone(),
    );
    
    // 4. 提交到 Validator 的 DAG
    self.submit_to_validator(event.clone()).await?;
    
    Ok(event)
}
```

## 状态持久化

当前使用 `InMemoryStateStore`，生产环境需要持久化存储：

```rust
// 未来实现
pub struct RocksDBStateStore {
    db: Arc<RocksDB>,
}

impl StateStore for RocksDBStateStore {
    fn get_object(&self, object_id: &ObjectId) -> RuntimeResult<Option<Object<CoinData>>> {
        // 从 RocksDB 读取
    }
    
    fn set_object(&mut self, object_id: ObjectId, object: Object<CoinData>) -> RuntimeResult<()> {
        // 写入 RocksDB
    }
    
    // ...
}

// 使用
let state = RocksDBStateStore::new(db_path)?;
let runtime = RuntimeExecutor::new(state);
```

## 与 TEE 集成

未来在 TEE 中执行时：

```rust
impl Executor {
    pub async fn execute_in_tee(
        &mut self,
        transfer: &Transfer,
    ) -> anyhow::Result<ExecutionOutput> {
        let tx = self.convert_transfer_to_tx(transfer)?;
        
        let ctx = ExecutionContext {
            executor_id: self.node_id.clone(),
            timestamp: current_timestamp(),
            in_tee: true, // 标记在 TEE 中执行
        };
        
        // 在 TEE enclave 中执行
        let output = tee::execute_in_enclave(|| {
            self.runtime.execute_transaction(&tx, &ctx)
        })?;
        
        // TEE 会额外返回执行证明
        let proof = tee::generate_proof(&output)?;
        
        Ok(output)
    }
}
```

## 迁移到 Move VM

当准备引入 Move VM 时，只需要：

1. 实现 `MoveVMExecutor`，使其具有相同的 `execute_transaction` 接口
2. 在 `Executor` 中替换执行引擎：

```rust
pub struct Executor {
    node_id: String,
    // 从 RuntimeExecutor 切换到 MoveVMExecutor
    runtime: MoveVMExecutor,
}
```

3. 其他代码（Solver、Validator、DAG 等）无需修改

这就是模块化设计的优势！



## Future Extensions

### 1. TEE Execution

```rust
impl ExecutionContext {
    pub async fn execute_in_tee<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        // Execute in SGX/TDX enclave
        // Return execution result + proof
    }
}
```

### 2. Move VM Integration

```rust
pub struct MoveVMExecutor {
    vm: MoveVM,
    state: Box<dyn StateStore>,
}

impl MoveVMExecutor {
    pub fn execute_transaction(&mut self, tx: &Transaction) -> Result<ExecutionOutput> {
        // Execute using Move VM
        let session = self.vm.new_session(&self.state);
        // ...
    }
}
```

### 3. More Transaction Types

- Coin Merge
- Object Delete
- Batch Operations
- Cross-Subnet Calls

## Testing

```bash
cd crates/setu-runtime
cargo test
```

## License

Same as the main project
