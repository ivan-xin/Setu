// storage/src/state/speculative_overlay.rs
//
// SpeculativeOverlay — 每事件作用域的推测覆盖层。
//
// 用于替代 MoveCall 的 pre-apply-to-SMT 模式：
//   - pre-apply 写 overlay（而非 SMT）
//   - CF 最终化后清理本 event_id 的 overlay 条目（无论 applied / stale_read）
//   - 读路径合并 overlay + SMT snapshot
//
// 详见 docs/feat/move-call-speculative-overlay/design.md。

use setu_merkle::HashValue;
use setu_types::{event::StateChange, SubnetId};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 单个 pre-apply TX 对一个 SMT key 的预写字节。
#[derive(Clone, Debug)]
pub(crate) struct SpeculativeEntry {
    /// 哪个 event 预写了此条 —— CF 清理时按此过滤。
    pub event_id: String,
    /// None = 预 Delete；Some = 预 Insert/Update。
    pub value: Option<Vec<u8>>,
    /// 入栈时刻，仅用于 `OverlayStats` 的 oldest_age。
    pub staged_at: Instant,
}

/// Per-Validator 的推测覆盖层。
///
/// # 不变量
/// - I1：`get` 永不写 SMT（本类型也不持有 SMT 句柄，从结构上保证）
/// - I3：每条 entry 携带 `event_id`，`clear_events` 按 event_id 批量过滤
/// - I4：只存在于单 Validator 内存；进程重启即清空
pub struct SpeculativeOverlay {
    /// 合成索引：(subnet, hash_key) → 最新的 SpeculativeEntry。
    /// 同一 key 被多笔 pre-apply 时，后来的覆盖前面的（entry.event_id 随之更新）。
    index: Mutex<HashMap<(SubnetId, HashValue), SpeculativeEntry>>,
}

/// `clear_events` 的返回统计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayClearStats {
    /// 本次清理成功移除的 overlay 条目数。
    pub cleared: usize,
}

/// 可观测指标。
#[derive(Debug, Clone)]
pub struct OverlayStats {
    pub entry_count: usize,
    pub oldest_age: Option<Duration>,
    pub unique_events: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum StageError {
    #[error("state change key '{0}' does not parse as oid:{{64-hex}}")]
    MalformedKey(String),
}

impl Default for SpeculativeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeculativeOverlay {
    pub fn new() -> Self {
        Self {
            index: Mutex::new(HashMap::new()),
        }
    }

    /// 读路径：查询单个 key。
    ///
    /// - `Some(Some(bytes))` → overlay 预写（Insert/Update）
    /// - `Some(None)`        → overlay 预 Delete（读者应视同"键不存在"）
    /// - `None`              → overlay 未覆盖，读者回落 SMT
    pub fn get(&self, subnet: &SubnetId, oid: &HashValue) -> Option<Option<Vec<u8>>> {
        let guard = self.index.lock().expect("overlay mutex poisoned");
        guard
            .get(&(*subnet, *oid))
            .map(|e| e.value.clone())
    }

    /// 预写：为单个 event 暂存一批变更。
    ///
    /// **原子性 (all-or-nothing)**：
    /// 1. 先遍历一次解析所有 `(subnet, HashValue, Option<Vec<u8>>)` 到临时 Vec；
    ///    非 `oid:` 前缀（`event:`、`user:`、`solver:`、`validator:`、`mod:` 等元数据 key）
    ///    直接跳过 —— 这些 key 不承载对象状态，不需要 read-your-writes。
    ///    `oid:` 前缀但 hex 非法返回 `Err(MalformedKey)`，不加锁、不改 index（G11 守门）
    /// 2. 全部解析通过后加锁，一次性批量插入
    ///
    /// key 前缀用 `change.target_subnet.unwrap_or(event_subnet)`（支持跨 subnet change）。
    /// 幂等：同一 event_id 多次 stage 同一 key 会覆盖（新字节生效，旧 entry 被替换）。
    pub fn stage(
        &self,
        event_id: &str,
        event_subnet: SubnetId,
        changes: &[StateChange],
    ) -> Result<(), StageError> {
        // Phase 1: parse everything into a staging vec without touching the index
        let mut staged: Vec<((SubnetId, HashValue), SpeculativeEntry)> =
            Vec::with_capacity(changes.len());
        let now = Instant::now();

        for change in changes {
            match parse_oid_hex_key(&change.key) {
                ParsedKey::Oid(hv) => {
                    let subnet = change.target_subnet.unwrap_or(event_subnet);
                    let entry = SpeculativeEntry {
                        event_id: event_id.to_string(),
                        value: change.new_value.clone(),
                        staged_at: now,
                    };
                    staged.push(((subnet, hv), entry));
                }
                ParsedKey::NonOid => {
                    // Metadata key (event:, user:, solver:, validator:, mod:, …).
                    // Does NOT participate in overlay — SMT is the sole writer.
                    continue;
                }
                ParsedKey::BadOid => {
                    return Err(StageError::MalformedKey(change.key.clone()));
                }
            }
        }

        // Phase 2: atomic bulk insert
        let mut guard = self.index.lock().expect("overlay mutex poisoned");
        for (k, v) in staged {
            guard.insert(k, v);
        }
        Ok(())
    }

    /// CF 最终化：清理所有 event_id 匹配的 overlay 条目。
    ///
    /// 无论 applied / stale_read 都统一丢弃：
    /// - applied：SMT 已吸收相同字节，清除后读路径无缝回落 SMT
    /// - stale_read：SMT 未变，清除后读路径暴露 SMT 原值 → 回滚完成
    ///
    /// 若 entry 已被后续 event 覆盖（`entry.event_id` 不在入参列表中）→ 保留，
    /// 由覆盖者自己的最终化负责清理。
    pub fn clear_events(&self, event_ids: &[String]) -> OverlayClearStats {
        if event_ids.is_empty() {
            return OverlayClearStats::default();
        }
        let filter: HashSet<&str> = event_ids.iter().map(String::as_str).collect();
        let mut guard = self.index.lock().expect("overlay mutex poisoned");
        let before = guard.len();
        guard.retain(|_, entry| !filter.contains(entry.event_id.as_str()));
        let after = guard.len();
        OverlayClearStats {
            cleared: before - after,
        }
    }

    /// 可观测：当前 overlay 中的条目数、最老条目的 age、去重 event 数。
    pub fn stats(&self) -> OverlayStats {
        let guard = self.index.lock().expect("overlay mutex poisoned");
        let entry_count = guard.len();
        let oldest_age = guard
            .values()
            .map(|e| e.staged_at)
            .min()
            .map(|t| t.elapsed());
        let unique_events: HashSet<&str> =
            guard.values().map(|e| e.event_id.as_str()).collect();
        OverlayStats {
            entry_count,
            oldest_age,
            unique_events: unique_events.len(),
        }
    }
}

/// 解析 `"oid:{64-hex}"` 形式的 state_change key 为 HashValue。
/// 返回 None 表示格式非法。
/// 解析 `"oid:{64-hex}"` 形式的 state_change key。
///
/// - `Oid(hv)` —— 合法 oid key，承载对象状态
/// - `NonOid`  —— 非 `oid:` 前缀（`event:` / `user:` / `solver:` / `validator:` / `mod:` 等
///                元数据 key），overlay 不管，SMT 自行处理
/// - `BadOid`  —— 以 `oid:` 开头但 hex 部分非法（长度不是 64 或含非 hex 字符）→
///                G11 违规，由调用者决定是否 fail-loud
#[derive(Debug, PartialEq, Eq)]
enum ParsedKey {
    Oid(HashValue),
    NonOid,
    BadOid,
}

fn parse_oid_hex_key(key: &str) -> ParsedKey {
    let hex_str = match key.strip_prefix("oid:") {
        Some(s) => s,
        None => return ParsedKey::NonOid,
    };
    if hex_str.len() != 64 {
        return ParsedKey::BadOid;
    }
    let mut bytes = [0u8; 32];
    if hex::decode_to_slice(hex_str, &mut bytes).is_err() {
        return ParsedKey::BadOid;
    }
    match HashValue::from_slice(&bytes) {
        Ok(hv) => ParsedKey::Oid(hv),
        Err(_) => ParsedKey::BadOid,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use setu_types::event::StateChange;
    use setu_types::SubnetId;

    /// 构造一个合法的 oid:{hex} key，以 byte `b` 填充 32 字节。
    fn oid_key(b: u8) -> (String, HashValue) {
        let bytes = [b; 32];
        (format!("oid:{}", hex::encode(bytes)), HashValue::from_slice(&bytes).unwrap())
    }

    #[test]
    fn new_is_empty() {
        let ov = SpeculativeOverlay::new();
        let s = ov.stats();
        assert_eq!(s.entry_count, 0);
        assert_eq!(s.unique_events, 0);
        assert!(s.oldest_age.is_none());
    }

    #[test]
    fn get_on_empty_returns_none() {
        let ov = SpeculativeOverlay::new();
        let (_, hv) = oid_key(0x11);
        assert!(ov.get(&SubnetId::ROOT, &hv).is_none());
    }

    #[test]
    fn stage_single_change_then_get() {
        let ov = SpeculativeOverlay::new();
        let (k, hv) = oid_key(0x22);
        let changes = vec![StateChange::insert(k, b"v1".to_vec())];
        ov.stage("E1", SubnetId::ROOT, &changes).unwrap();
        assert_eq!(ov.get(&SubnetId::ROOT, &hv), Some(Some(b"v1".to_vec())));
    }

    #[test]
    fn stage_delete_change_then_get() {
        let ov = SpeculativeOverlay::new();
        let (k, hv) = oid_key(0x33);
        let changes = vec![StateChange::delete(k, b"old".to_vec())];
        ov.stage("E1", SubnetId::ROOT, &changes).unwrap();
        assert_eq!(ov.get(&SubnetId::ROOT, &hv), Some(None));
    }

    #[test]
    fn stage_malformed_oid_key_rejected() {
        let ov = SpeculativeOverlay::new();
        // Starts with "oid:" but hex part is wrong length — real G11 violation.
        let bad = StateChange::insert("oid:abc".to_string(), b"v".to_vec());
        let err = ov.stage("E1", SubnetId::ROOT, &[bad]).unwrap_err();
        assert!(matches!(err, StageError::MalformedKey(_)));
        assert_eq!(ov.stats().entry_count, 0);
    }

    #[test]
    fn stage_non_oid_keys_are_skipped() {
        // Metadata keys (event:, user:, …) are legitimate state-change outputs
        // from the Move VM / TEE but do NOT carry object state and must NOT
        // flow through the overlay. They should be silently skipped, NOT
        // treated as G11 violations.
        let ov = SpeculativeOverlay::new();
        let (k, hv) = oid_key(0x90);
        let changes = vec![
            StateChange::insert(k, b"obj".to_vec()),
            StateChange::insert("event:abc123".to_string(), b"meta1".to_vec()),
            StateChange::insert("user:0x...".to_string(), b"meta2".to_vec()),
            StateChange::insert("mod:cafe::module".to_string(), b"meta3".to_vec()),
        ];
        ov.stage("E1", SubnetId::ROOT, &changes).expect("non-oid keys must not error");
        // Only the oid: change landed in the overlay.
        assert_eq!(ov.stats().entry_count, 1);
        assert_eq!(ov.get(&SubnetId::ROOT, &hv), Some(Some(b"obj".to_vec())));
    }

    #[test]
    fn stage_partial_malformed_aborts_all() {
        let ov = SpeculativeOverlay::new();
        let (k1, _) = oid_key(0x44);
        let (k3, _) = oid_key(0x45);
        let changes = vec![
            StateChange::insert(k1, b"v1".to_vec()),
            // Truly malformed oid key — starts with "oid:" but hex wrong.
            StateChange::insert("oid:nothex".to_string(), b"v2".to_vec()),
            StateChange::insert(k3, b"v3".to_vec()),
        ];
        let err = ov.stage("E1", SubnetId::ROOT, &changes).unwrap_err();
        assert!(matches!(err, StageError::MalformedKey(_)));
        // all-or-nothing: nothing in the index
        assert_eq!(ov.stats().entry_count, 0);
    }

    #[test]
    fn stage_same_key_overwrites_event_id() {
        let ov = SpeculativeOverlay::new();
        let (k, hv) = oid_key(0x55);
        ov.stage("E1", SubnetId::ROOT, &[StateChange::insert(k.clone(), b"v1".to_vec())])
            .unwrap();
        ov.stage("E3", SubnetId::ROOT, &[StateChange::insert(k, b"v3".to_vec())])
            .unwrap();
        // get returns the latest
        assert_eq!(ov.get(&SubnetId::ROOT, &hv), Some(Some(b"v3".to_vec())));
        // clearing E1 is now a no-op (entry.event_id == "E3")
        let stats = ov.clear_events(&["E1".to_string()]);
        assert_eq!(stats.cleared, 0);
        assert_eq!(ov.stats().entry_count, 1);
    }

    #[test]
    fn stage_respects_target_subnet() {
        let ov = SpeculativeOverlay::new();
        let (k, hv) = oid_key(0x66);
        let change = StateChange::insert(k, b"v".to_vec())
            .with_target_subnet(SubnetId::GOVERNANCE);
        // event_subnet = ROOT, but target_subnet = GOVERNANCE
        ov.stage("E1", SubnetId::ROOT, &[change]).unwrap();
        assert_eq!(
            ov.get(&SubnetId::GOVERNANCE, &hv),
            Some(Some(b"v".to_vec()))
        );
        assert!(ov.get(&SubnetId::ROOT, &hv).is_none());
    }

    #[test]
    fn clear_events_removes_matched_only() {
        let ov = SpeculativeOverlay::new();
        let (k1, _) = oid_key(0x77);
        let (k2, _) = oid_key(0x78);
        ov.stage("E1", SubnetId::ROOT, &[StateChange::insert(k1, b"v1".to_vec())])
            .unwrap();
        ov.stage("E2", SubnetId::ROOT, &[StateChange::insert(k2, b"v2".to_vec())])
            .unwrap();
        let stats = ov.clear_events(&["E1".to_string()]);
        assert_eq!(stats.cleared, 1);
        assert_eq!(ov.stats().entry_count, 1);
    }

    #[test]
    fn clear_events_no_match_is_noop() {
        let ov = SpeculativeOverlay::new();
        let (k, _) = oid_key(0x88);
        ov.stage("E3", SubnetId::ROOT, &[StateChange::insert(k, b"v".to_vec())])
            .unwrap();
        let stats = ov.clear_events(&["E1".to_string()]);
        assert_eq!(stats.cleared, 0);
        assert_eq!(ov.stats().entry_count, 1);
    }

    #[test]
    fn clear_events_empty_list() {
        let ov = SpeculativeOverlay::new();
        let (k, _) = oid_key(0x99);
        ov.stage("E1", SubnetId::ROOT, &[StateChange::insert(k, b"v".to_vec())])
            .unwrap();
        let stats = ov.clear_events(&[]);
        assert_eq!(stats.cleared, 0);
        assert_eq!(ov.stats().entry_count, 1);
    }

    #[test]
    fn stats_tracks_unique_events() {
        let ov = SpeculativeOverlay::new();
        let (k1, _) = oid_key(0xA1);
        let (k2, _) = oid_key(0xA2);
        let (k3, _) = oid_key(0xA3);
        ov.stage(
            "E1",
            SubnetId::ROOT,
            &[
                StateChange::insert(k1, b"v".to_vec()),
                StateChange::insert(k2, b"v".to_vec()),
            ],
        )
        .unwrap();
        ov.stage("E2", SubnetId::ROOT, &[StateChange::insert(k3, b"v".to_vec())])
            .unwrap();
        let s = ov.stats();
        assert_eq!(s.entry_count, 3);
        assert_eq!(s.unique_events, 2);
    }

    #[test]
    fn stats_oldest_age_reflects_insertion() {
        let ov = SpeculativeOverlay::new();
        let (k, _) = oid_key(0xBB);
        ov.stage("E1", SubnetId::ROOT, &[StateChange::insert(k, b"v".to_vec())])
            .unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let age = ov.stats().oldest_age.expect("should have entry");
        assert!(age >= Duration::from_millis(10));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_stage_and_get_no_race() {
        use std::sync::Arc;
        let ov = Arc::new(SpeculativeOverlay::new());
        let mut handles = Vec::new();

        // 4 writer tasks, each staging 1000 distinct keys under E{i}
        for i in 0u8..4 {
            let ov_c = Arc::clone(&ov);
            handles.push(tokio::spawn(async move {
                for n in 0u16..1000 {
                    // unique byte pattern per (i, n): (i, n_hi, n_lo, padding...)
                    let mut bytes = [0u8; 32];
                    bytes[0] = i;
                    bytes[1] = (n >> 8) as u8;
                    bytes[2] = (n & 0xff) as u8;
                    let key = format!("oid:{}", hex::encode(bytes));
                    let ch = StateChange::insert(key, vec![i, n as u8]);
                    ov_c.stage(&format!("E{}", i), SubnetId::ROOT, &[ch])
                        .unwrap();
                }
            }));
        }
        // 4 reader tasks, each probing 1000 random keys
        for _ in 0..4 {
            let ov_c = Arc::clone(&ov);
            handles.push(tokio::spawn(async move {
                for _ in 0..1000 {
                    let hv = HashValue::from_slice(&[0u8; 32]).unwrap();
                    let _ = ov_c.get(&SubnetId::ROOT, &hv);
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let s = ov.stats();
        assert_eq!(s.entry_count, 4 * 1000);
        assert_eq!(s.unique_events, 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_stage_and_clear_no_loss() {
        use std::sync::Arc;
        let ov = Arc::new(SpeculativeOverlay::new());

        // Stage 10 events, 1 key each.
        for n in 0u8..10 {
            let mut bytes = [0u8; 32];
            bytes[0] = n;
            let key = format!("oid:{}", hex::encode(bytes));
            ov.stage(
                &format!("E{}", n),
                SubnetId::ROOT,
                &[StateChange::insert(key, vec![n])],
            )
            .unwrap();
        }
        assert_eq!(ov.stats().entry_count, 10);

        // Concurrently clear the even-numbered events from two tasks.
        let to_clear: Vec<String> = (0u8..10).filter(|n| n % 2 == 0).map(|n| format!("E{}", n)).collect();
        let ov_a = Arc::clone(&ov);
        let to_clear_a = to_clear.clone();
        let ov_b = Arc::clone(&ov);
        let to_clear_b = to_clear.clone();
        let ha = tokio::spawn(async move { ov_a.clear_events(&to_clear_a) });
        let hb = tokio::spawn(async move { ov_b.clear_events(&to_clear_b) });
        let sa = ha.await.unwrap();
        let sb = hb.await.unwrap();
        // Combined clears must equal 5 (5 even events); idempotent — second task sees 0.
        assert_eq!(sa.cleared + sb.cleared, 5);
        assert_eq!(ov.stats().entry_count, 5);

        // Surviving entries are odd-indexed
        let surviving = ov.stats().unique_events;
        assert_eq!(surviving, 5);
    }
}
