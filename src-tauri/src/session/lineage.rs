//! 分页 rollout 的多段续写（lineage）。
//!
//! Codex 0.154+ 会在会话运行过程中把同一个线程切到新的物理 rollout 文件：
//! 文件名形如 `rollout-<时间>-<线程 id>_<段 id>.jsonl`，首行 `session_meta.payload.id`
//! 仍是逻辑线程 id，`history_base` 指向上一段的物理段 id 以及在上一段中继承的
//! 前缀边界（`end_byte_offset` / `end_ordinal_exclusive`）。首段没有后缀，物理段 id
//! 与逻辑线程 id 相同。
//!
//! Codex 读取会话时把各段按链拼接：前面各段只取到被后继段引用的边界为止，链尾
//! 才是当前写入段；分页历史投影（thread_history）也是按物理段 id 分别记录的。
//! 因此 Switcher 在列出会话、查看内容、重建投影和改写文件时都必须按 lineage 处理，
//! 而不能把最大的那个文件当作整个会话。

use super::RolloutHistoryBase;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

/// 扫描阶段收集到的单个 rollout 文件信息。
#[derive(Debug, Clone)]
pub(super) struct SegmentInput {
    pub path: PathBuf,
    pub base: Option<RolloutHistoryBase>,
    pub file_len: u64,
    pub updated_at: i64,
}

/// lineage 中的一个物理段。
#[derive(Debug, Clone)]
pub(super) struct RolloutSegment {
    pub physical_id: String,
    pub path: PathBuf,
    pub file_len: u64,
    pub updated_at: i64,
    /// 被后继段引用时保留的前缀：(字节数, ordinal 上界 exclusive)。链尾为 None。
    pub retained: Option<(u64, u64)>,
}

impl RolloutSegment {
    /// 该段对整个会话有效的字节数：被引用段只算前缀，链尾算全文件。
    pub fn effective_len(&self) -> u64 {
        self.retained
            .map(|(bytes, _)| bytes.min(self.file_len))
            .unwrap_or(self.file_len)
    }
}

/// 同一逻辑线程从首段到当前写入段的完整链。
#[derive(Debug, Clone)]
pub(super) struct RolloutLineage {
    pub logical_id: String,
    /// 首段在前，链尾（当前写入段）在后，至少一个元素。
    pub segments: Vec<RolloutSegment>,
}

impl RolloutLineage {
    /// 当前写入段（链尾）。
    pub fn head(&self) -> &RolloutSegment {
        self.segments.last().expect("lineage 至少包含一个物理段")
    }

    pub fn is_split(&self) -> bool {
        self.segments.len() > 1
    }

    pub fn total_bytes(&self) -> u64 {
        self.segments
            .iter()
            .map(RolloutSegment::effective_len)
            .sum()
    }

    pub fn updated_at(&self) -> i64 {
        self.segments
            .iter()
            .map(|segment| segment.updated_at)
            .max()
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn physical_ids(&self) -> impl Iterator<Item = &str> {
        self.segments
            .iter()
            .map(|segment| segment.physical_id.as_str())
    }

    /// 唯一指纹：任一段的路径、有效长度或修改时间变化都会得到不同值，用于缓存文件名。
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        for segment in &self.segments {
            hasher.update(segment.path.to_string_lossy().as_bytes());
            hasher.update(segment.effective_len().to_le_bytes());
            hasher.update(segment.updated_at.to_le_bytes());
        }
        hasher
            .finalize()
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

fn looks_like_thread_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        && value.matches('-').count() == 4
}

/// 从文件名推断物理段 id：`..._<段 id>.jsonl` 取后缀，否则就是逻辑线程 id 本身。
pub(super) fn physical_rollout_id(path: &Path, logical_id: &str) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if let Some((prefix, suffix)) = stem.rsplit_once('_') {
        if looks_like_thread_id(suffix) && prefix.ends_with(logical_id) && suffix != logical_id {
            return suffix.to_string();
        }
    }
    logical_id.to_string()
}

fn more_complete(candidate: &SegmentInput, current: &SegmentInput) -> bool {
    candidate.file_len > current.file_len
        || (candidate.file_len == current.file_len
            && (candidate.updated_at > current.updated_at
                || (candidate.updated_at == current.updated_at && candidate.path > current.path)))
}

/// 把同一逻辑线程的所有文件整理成 lineage。
///
/// 返回主链以及未能接入主链的文件（例如手动复制出来的重复文件）。没有任何
/// `history_base` 关系时退化为旧规则：取最大、最新的文件作为唯一段。
pub(super) fn resolve_lineage(
    logical_id: &str,
    inputs: Vec<SegmentInput>,
) -> Option<(RolloutLineage, Vec<PathBuf>)> {
    if inputs.is_empty() {
        return None;
    }
    // 各物理段被后继段引用时要求的最小前缀长度，用来在同名冲突时识别真正的源文件。
    let mut required_prefix = HashMap::<String, u64>::new();
    for base in inputs.iter().filter_map(|input| input.base.as_ref()) {
        let slot = required_prefix.entry(base.thread_id.clone()).or_insert(0);
        *slot = (*slot).max(base.end_byte_offset);
    }
    // 同一物理段 id 出现多个文件时，优先保留能满足后继段前缀要求的文件，其次最完整的那个。
    let mut by_physical = HashMap::<String, SegmentInput>::new();
    let mut orphans = Vec::new();
    for input in inputs {
        let physical_id = physical_rollout_id(&input.path, logical_id);
        let satisfies = |candidate: &SegmentInput| {
            required_prefix
                .get(&physical_id)
                .is_none_or(|bytes| candidate.file_len >= *bytes)
        };
        match by_physical.get(&physical_id) {
            Some(existing) => {
                let replace = match (satisfies(&input), satisfies(existing)) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => more_complete(&input, existing),
                };
                if replace {
                    orphans.push(existing.path.clone());
                    by_physical.insert(physical_id, input);
                } else {
                    orphans.push(input.path);
                }
            }
            None => {
                by_physical.insert(physical_id, input);
            }
        }
    }

    // 被其他段作为 history_base 引用的物理段不可能是链尾。
    let referenced = by_physical
        .values()
        .filter_map(|input| input.base.as_ref())
        .filter(|base| by_physical.contains_key(&base.thread_id))
        .map(|base| base.thread_id.clone())
        .collect::<HashSet<_>>();
    let chain_from = |tail: &str| -> Vec<String> {
        let mut chain = vec![tail.to_string()];
        let mut visited = HashSet::from([tail.to_string()]);
        let mut current = tail.to_string();
        while let Some(base) = by_physical
            .get(&current)
            .and_then(|input| input.base.as_ref())
        {
            if !by_physical.contains_key(&base.thread_id) || !visited.insert(base.thread_id.clone())
            {
                break;
            }
            chain.push(base.thread_id.clone());
            current = base.thread_id.clone();
        }
        chain
    };
    // 候选链尾：链最长者优先，其次更新时间、文件大小。
    let mut tails = by_physical
        .keys()
        .filter(|id| !referenced.contains(*id))
        .map(|id| (chain_from(id), id.clone()))
        .collect::<Vec<_>>();
    if tails.is_empty() {
        // 只有环时没有链尾，退化为最完整的文件。
        let best = by_physical
            .iter()
            .max_by(|(_, a), (_, b)| {
                if more_complete(a, b) {
                    std::cmp::Ordering::Greater
                } else if more_complete(b, a) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .map(|(id, _)| id.clone())?;
        tails.push((vec![best.clone()], best));
    }
    tails.sort_by(|(chain_a, id_a), (chain_b, id_b)| {
        let input_a = &by_physical[id_a];
        let input_b = &by_physical[id_b];
        chain_b.len().cmp(&chain_a.len()).then_with(|| {
            if more_complete(input_a, input_b) {
                std::cmp::Ordering::Less
            } else if more_complete(input_b, input_a) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    let (chain, _) = tails.remove(0);
    let in_chain = chain.iter().cloned().collect::<HashSet<_>>();

    // chain 是从链尾往回的顺序，翻转成首段在前。
    let mut segments = Vec::with_capacity(chain.len());
    let mut retained_for_previous: Option<(u64, u64)> = None;
    for physical_id in chain.iter() {
        let input = by_physical[physical_id].clone();
        let retained = retained_for_previous.take();
        retained_for_previous = input
            .base
            .as_ref()
            .filter(|base| in_chain.contains(&base.thread_id))
            .map(|base| (base.end_byte_offset, base.end_ordinal_exclusive));
        segments.push(RolloutSegment {
            physical_id: physical_id.clone(),
            path: input.path,
            file_len: input.file_len,
            updated_at: input.updated_at,
            retained,
        });
    }
    segments.reverse();
    for (physical_id, input) in by_physical {
        if !in_chain.contains(&physical_id) {
            orphans.push(input.path);
        }
    }
    orphans.sort();
    Some((
        RolloutLineage {
            logical_id: logical_id.to_string(),
            segments,
        },
        orphans,
    ))
}

/// 把多段 lineage 拼接成一个只读的 JSONL 文件供查看/搜索使用；单段直接返回原路径。
///
/// 缓存文件按指纹命名，任一段变化都会重新生成，同一线程的旧缓存随之删除。
pub(super) fn materialize_lineage(
    lineage: &RolloutLineage,
    cache_dir: &Path,
) -> Result<PathBuf, String> {
    if !lineage.is_split() {
        return Ok(lineage.head().path.clone());
    }
    fs::create_dir_all(cache_dir).map_err(|error| {
        format!(
            "创建会话合并缓存目录失败 ({}): {error}",
            cache_dir.display()
        )
    })?;
    let prefix = format!("{}-", lineage.logical_id);
    let target = cache_dir.join(format!("{prefix}{}.jsonl", lineage.fingerprint()));
    if target.is_file() {
        return Ok(target);
    }
    // 清理同一线程的过期缓存。
    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name
                .to_str()
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".jsonl"))
            {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let tmp = cache_dir.join(format!(
        ".{prefix}{}.{:016x}.tmp",
        lineage.fingerprint(),
        rand::random::<u64>()
    ));
    let result = (|| -> Result<(), String> {
        let mut writer = std::io::BufWriter::new(
            fs::File::create(&tmp)
                .map_err(|error| format!("创建会话合并缓存失败 ({}): {error}", tmp.display()))?,
        );
        for segment in &lineage.segments {
            let file = fs::File::open(&segment.path).map_err(|error| {
                format!("读取会话分段失败 ({}): {error}", segment.path.display())
            })?;
            let mut reader = file.take(segment.effective_len());
            let mut buffer = [0u8; 64 * 1024];
            let mut last_byte = b'\n';
            loop {
                let read = reader.read(&mut buffer).map_err(|error| {
                    format!("读取会话分段失败 ({}): {error}", segment.path.display())
                })?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|error| format!("写入会话合并缓存失败: {error}"))?;
                last_byte = buffer[read - 1];
            }
            // 边界理论上落在行尾；防御性补一个换行，避免下一段首行被粘连。
            if last_byte != b'\n' {
                writer
                    .write_all(b"\n")
                    .map_err(|error| format!("写入会话合并缓存失败: {error}"))?;
            }
        }
        writer
            .flush()
            .map_err(|error| format!("写入会话合并缓存失败: {error}"))
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    fs::rename(&tmp, &target).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        format!("保存会话合并缓存失败 ({}): {error}", target.display())
    })?;
    Ok(target)
}

/// 单个物理段在分页历史投影里应达到的状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SegmentProjectionTarget {
    pub physical_id: String,
    /// 该段是否被后继段引用。Codex 读取时按继承边界过滤被引用段的投影，而且切段前
    /// 已把它投影到文件末尾，所以这类段的投影不能按"到边界为止"去校对或重置。
    pub retained: bool,
    pub next_byte_offset: i64,
    pub next_ordinal: i64,
    pub turn_count: usize,
}

/// 按物理段计算投影目标：被引用段截止在继承边界，链尾到文件末尾。非分页 rollout 返回空。
pub(super) fn lineage_projection_targets(
    lineage: &RolloutLineage,
) -> Result<Vec<SegmentProjectionTarget>, String> {
    let mut targets = Vec::with_capacity(lineage.segments.len());
    for segment in &lineage.segments {
        let file = fs::File::open(&segment.path).map_err(|error| {
            format!("读取分页会话文件失败 ({}): {error}", segment.path.display())
        })?;
        let limit = segment.effective_len();
        let mut reader = BufReader::new(file.take(limit));
        let mut line = String::new();
        let mut first_record = true;
        let mut last_ordinal = None;
        let mut turn_count = 0usize;
        loop {
            line.clear();
            let read = reader.read_line(&mut line).map_err(|error| {
                format!("读取分页会话记录失败 ({}): {error}", segment.path.display())
            })?;
            if read == 0 {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str::<serde_json::Value>(&line).map_err(|error| {
                format!("解析分页会话记录失败 ({}): {error}", segment.path.display())
            })?;
            if first_record {
                first_record = false;
                if value.get("type").and_then(|v| v.as_str()) != Some("session_meta")
                    || value.get("ordinal").and_then(|v| v.as_u64()).is_none()
                {
                    return Ok(Vec::new());
                }
            }
            let ordinal = value
                .get("ordinal")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("分页会话记录缺少 ordinal ({})", segment.path.display()))?;
            last_ordinal = Some(ordinal);
            if value.get("type").and_then(|v| v.as_str()) == Some("event_msg")
                && value
                    .get("payload")
                    .and_then(|payload| payload.get("type"))
                    .and_then(|v| v.as_str())
                    == Some("task_started")
            {
                turn_count = turn_count.saturating_add(1);
            }
        }
        let Some(last_ordinal) = last_ordinal else {
            return Ok(Vec::new());
        };
        let (next_byte_offset, next_ordinal) = match segment.retained {
            Some((bytes, end_ordinal_exclusive)) => {
                (bytes.min(segment.file_len), end_ordinal_exclusive)
            }
            None => (segment.file_len, last_ordinal.saturating_add(1)),
        };
        targets.push(SegmentProjectionTarget {
            physical_id: segment.physical_id.clone(),
            retained: segment.retained.is_some(),
            next_byte_offset: i64::try_from(next_byte_offset).map_err(|_| {
                format!(
                    "分页会话文件过大，无法重建历史投影 ({})",
                    segment.path.display()
                )
            })?,
            next_ordinal: i64::try_from(next_ordinal)
                .map_err(|_| format!("分页会话 ordinal 超出范围 ({})", segment.path.display()))?,
            turn_count,
        });
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(thread_id: &str, bytes: u64, ordinal: u64) -> RolloutHistoryBase {
        RolloutHistoryBase {
            thread_id: thread_id.to_string(),
            end_byte_offset: bytes,
            end_ordinal_exclusive: ordinal,
        }
    }

    const LOGICAL: &str = "01a06a6e-489b-7953-b7a2-46c08f772a12";
    const SEG2: &str = "01a09f06-f038-70e1-8cfc-ba9feba3c47e";
    const SEG3: &str = "01a0a3b1-68ba-7143-ae37-3d9a01084c57";

    #[test]
    fn derives_physical_id_from_file_suffix() {
        let first = PathBuf::from(format!("/s/rollout-2026-09-04T11-20-07-{LOGICAL}.jsonl"));
        assert_eq!(physical_rollout_id(&first, LOGICAL), LOGICAL);
        let second = PathBuf::from(format!(
            "/s/rollout-2026-09-14T16-27-07-{LOGICAL}_{SEG2}.jsonl"
        ));
        assert_eq!(physical_rollout_id(&second, LOGICAL), SEG2);
        // 下划线后不是线程 id 形态的文件名不算续段。
        let other = PathBuf::from("/s/rollout-copy_backup.jsonl");
        assert_eq!(physical_rollout_id(&other, LOGICAL), LOGICAL);
    }

    #[test]
    fn resolves_multi_segment_chain_with_retained_prefixes() {
        let inputs = vec![
            SegmentInput {
                path: PathBuf::from(format!(
                    "/s/rollout-2026-09-15T14-11-48-{LOGICAL}_{SEG3}.jsonl"
                )),
                base: Some(base(SEG2, 50_576, 18_437)),
                file_len: 2_840_422,
                updated_at: 300,
            },
            SegmentInput {
                path: PathBuf::from(format!("/s/rollout-2026-09-04T11-20-07-{LOGICAL}.jsonl")),
                base: None,
                file_len: 89_468_323,
                updated_at: 100,
            },
            SegmentInput {
                path: PathBuf::from(format!(
                    "/s/rollout-2026-09-14T16-27-07-{LOGICAL}_{SEG2}.jsonl"
                )),
                base: Some(base(LOGICAL, 89_438_285, 5_917)),
                file_len: 4_100_050,
                updated_at: 200,
            },
        ];
        let (lineage, orphans) = resolve_lineage(LOGICAL, inputs).expect("lineage");
        assert!(orphans.is_empty());
        assert!(lineage.is_split());
        let ids = lineage.physical_ids().collect::<Vec<_>>();
        assert_eq!(ids, vec![LOGICAL, SEG2, SEG3]);
        assert_eq!(lineage.segments[0].retained, Some((89_438_285, 5_917)));
        assert_eq!(lineage.segments[1].retained, Some((50_576, 18_437)));
        assert_eq!(lineage.segments[2].retained, None);
        assert_eq!(lineage.total_bytes(), 89_438_285 + 50_576 + 2_840_422);
        assert!(lineage.head().path.to_string_lossy().contains(SEG3));
        assert_eq!(lineage.updated_at(), 300);
    }

    #[test]
    fn falls_back_to_most_complete_file_without_history_base() {
        let inputs = vec![
            SegmentInput {
                path: PathBuf::from("/s/a.jsonl"),
                base: None,
                file_len: 10,
                updated_at: 5,
            },
            SegmentInput {
                path: PathBuf::from("/s/b.jsonl"),
                base: None,
                file_len: 20,
                updated_at: 1,
            },
        ];
        let (lineage, orphans) = resolve_lineage(LOGICAL, inputs).expect("lineage");
        assert!(!lineage.is_split());
        assert_eq!(lineage.head().path, PathBuf::from("/s/b.jsonl"));
        assert_eq!(orphans, vec![PathBuf::from("/s/a.jsonl")]);
    }

    #[test]
    fn prefers_the_longest_chain_over_a_bigger_orphan() {
        let inputs = vec![
            SegmentInput {
                path: PathBuf::from(format!("/s/rollout-x-{LOGICAL}.jsonl")),
                base: None,
                file_len: 1_000,
                updated_at: 1,
            },
            SegmentInput {
                path: PathBuf::from(format!("/s/rollout-y-{LOGICAL}_{SEG2}.jsonl")),
                base: Some(base(LOGICAL, 900, 10)),
                file_len: 50,
                updated_at: 2,
            },
            // 同一线程 id、无后缀的手工副本：比首段新但长度不够覆盖后继段引用的前缀，
            // 不能替代真正的首段，应被视为孤儿。
            SegmentInput {
                path: PathBuf::from("/s/manual-copy.jsonl"),
                base: None,
                file_len: 500,
                updated_at: 9,
            },
        ];
        let (lineage, orphans) = resolve_lineage(LOGICAL, inputs).expect("lineage");
        assert_eq!(
            lineage.physical_ids().collect::<Vec<_>>(),
            vec![LOGICAL, SEG2]
        );
        assert_eq!(orphans, vec![PathBuf::from("/s/manual-copy.jsonl")]);
    }

    #[test]
    fn materializes_segments_up_to_retained_prefix_and_computes_projection_targets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = dir.path().join(format!("rollout-a-{LOGICAL}.jsonl"));
        let line = |ordinal: u64, body: &str| format!("{{\"ordinal\":{ordinal},{body}}}\n");
        let first_lines = [
            line(0, "\"type\":\"session_meta\",\"payload\":{\"id\":\"x\"}"),
            line(
                1,
                "\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}",
            ),
            line(
                2,
                "\"type\":\"response_item\",\"payload\":{\"type\":\"message\"}",
            ),
            // 这一行超出继承边界，不应出现在合并结果里，也不计入投影。
            line(
                3,
                "\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}",
            ),
        ];
        let cutoff = first_lines[..3].iter().map(String::len).sum::<usize>() as u64;
        fs::write(&first, first_lines.concat()).unwrap();
        let second = dir.path().join(format!("rollout-b-{LOGICAL}_{SEG2}.jsonl"));
        let second_lines = [
            line(3, "\"type\":\"session_meta\",\"payload\":{\"id\":\"x\"}"),
            line(
                4,
                "\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}",
            ),
        ];
        fs::write(&second, second_lines.concat()).unwrap();
        let inputs = vec![
            SegmentInput {
                path: first.clone(),
                base: None,
                file_len: fs::metadata(&first).unwrap().len(),
                updated_at: 1,
            },
            SegmentInput {
                path: second.clone(),
                base: Some(base(LOGICAL, cutoff, 3)),
                file_len: fs::metadata(&second).unwrap().len(),
                updated_at: 2,
            },
        ];
        let (lineage, _) = resolve_lineage(LOGICAL, inputs).expect("lineage");

        let cache = dir.path().join("cache");
        let merged = materialize_lineage(&lineage, &cache).expect("materialize");
        let content = fs::read_to_string(&merged).unwrap();
        let ordinals = content
            .lines()
            .map(|l| {
                serde_json::from_str::<serde_json::Value>(l).unwrap()["ordinal"]
                    .as_u64()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(ordinals, vec![0, 1, 2, 3, 4]);
        // 再次调用命中缓存，返回同一路径。
        assert_eq!(materialize_lineage(&lineage, &cache).unwrap(), merged);

        let targets = lineage_projection_targets(&lineage).expect("targets");
        assert_eq!(
            targets,
            vec![
                SegmentProjectionTarget {
                    physical_id: LOGICAL.to_string(),
                    retained: true,
                    next_byte_offset: cutoff as i64,
                    next_ordinal: 3,
                    turn_count: 1,
                },
                SegmentProjectionTarget {
                    physical_id: SEG2.to_string(),
                    retained: false,
                    next_byte_offset: fs::metadata(&second).unwrap().len() as i64,
                    next_ordinal: 5,
                    turn_count: 1,
                },
            ]
        );
    }
}
