//! Overlap detection (research.md §3): deterministic, offline, lexical. Two mechanisms.
//!
//! 1. **Duplicate-by-identity** (FR-008): skills sharing a `content_hash` and/or the same
//!    `id` across sources. Exact, cheap, unambiguous.
//! 2. **Similarity** (FR-006/007): cluster `name + description` by a blend of token-set
//!    Jaccard and TF-IDF cosine above a tuned threshold; each cluster carries a
//!    human-readable `reason` and `score`. No model, no network — fully deterministic.
//!
//! Advisory only (FR-009): clustering never triggers an action. No singletons; unrelated
//! skills are never force-grouped (FR-010).

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::model::{ClusterKind, OverlapCluster, Skill};

/// Combined-similarity threshold. Tuned so the code-review family clusters while the
/// playwright / standalone skills stay out (see tests/overlap_clusters.rs).
const SIM_THRESHOLD: f64 = 0.30;

/// Tiny stopword set so structural words don't manufacture similarity.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "run", "write", "this", "that", "from", "into", "over", "across",
    "your", "you",
];

/// Detect all clusters for an inventory, in deterministic order: duplicate-identity first,
/// then similarity, each sorted by cluster id. Pairs already grouped by identity are excluded
/// from the similarity pass so an identical pair is reported once, as duplicate-identity.
pub fn detect(skills: &[Skill]) -> Vec<OverlapCluster> {
    let identity = build_identity(skills);
    let mut clusters = identity_clusters(skills, &identity);
    clusters.extend(similarity_clusters(skills, &identity));
    clusters
}

/// A non-null human reason for an empty cluster set (FR-010).
pub fn empty_reason() -> String {
    "no overlaps found".to_string()
}

// ---- duplicate-by-identity -------------------------------------------------------------

/// Union-find over skills where two are "identical" if they share a content hash or the same
/// skill id across sources (FR-008). Built once; reused to gate the similarity pass.
fn build_identity(skills: &[Skill]) -> UnionFind {
    let n = skills.len();
    let mut uf = UnionFind::new(n);
    let mut by_hash: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, s) in skills.iter().enumerate() {
        by_hash.entry(s.content_hash.as_str()).or_default().push(i);
    }
    for idxs in by_hash.values() {
        for w in idxs.windows(2) {
            uf.union(w[0], w[1]);
        }
    }
    let mut by_id: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, s) in skills.iter().enumerate() {
        by_id.entry(s.id.as_str()).or_default().push(i);
    }
    for idxs in by_id.values() {
        for w in idxs.windows(2) {
            uf.union(w[0], w[1]);
        }
    }
    uf
}

fn identity_clusters(skills: &[Skill], identity: &UnionFind) -> Vec<OverlapCluster> {
    let keys: Vec<String> = skills.iter().map(|s| s.key()).collect();
    let n = skills.len();
    let mut uf = identity.clone();

    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        groups.entry(uf.find(i)).or_default().push(i);
    }

    let mut out = Vec::new();
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let mut member_keys: Vec<String> = members.iter().map(|&i| keys[i].clone()).collect();
        member_keys.sort();
        let all_same_hash = members
            .iter()
            .map(|&i| skills[i].content_hash.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == 1;
        let reason = if all_same_hash {
            "identical content hash".to_string()
        } else {
            format!(
                "same skill id `{}` installed under {} sources",
                skills[members[0]].id,
                members.len()
            )
        };
        out.push(OverlapCluster {
            id: cluster_id(&member_keys),
            kind: ClusterKind::DuplicateIdentity,
            members: member_keys,
            reason,
            score: None,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

// ---- similarity ------------------------------------------------------------------------

struct Doc {
    key: String,
    tokens: Vec<String>,
    set: BTreeSet<String>,
}

fn similarity_clusters(skills: &[Skill], identity: &UnionFind) -> Vec<OverlapCluster> {
    // Map each skill index to its doc index (skills without usable text are excluded), so we
    // can consult the identity grouping (indexed by skill) while clustering docs.
    let mut skill_idx_of_doc: Vec<usize> = Vec::new();
    // Only skills with usable text participate.
    let mut docs: Vec<Doc> = Vec::new();
    for (si, s) in skills.iter().enumerate() {
        let text = match (&s.name, &s.description) {
            (n, d) if n.is_some() || d.is_some() => format!(
                "{} {}",
                n.clone().unwrap_or_default(),
                d.clone().unwrap_or_default()
            ),
            _ => continue,
        };
        let tokens = tokenize(&text);
        if tokens.is_empty() {
            continue;
        }
        let set = tokens.iter().cloned().collect();
        docs.push(Doc {
            key: s.key(),
            tokens,
            set,
        });
        skill_idx_of_doc.push(si);
    }

    let n = docs.len();
    if n < 2 {
        return Vec::new();
    }

    let mut identity = identity.clone();
    let idf = compute_idf(&docs);
    let mut uf = UnionFind::new(n);
    let mut sims: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for i in 0..n {
        for j in (i + 1)..n {
            // Skip pairs already grouped by identity — reported as duplicate-identity instead.
            if identity.connected(skill_idx_of_doc[i], skill_idx_of_doc[j]) {
                continue;
            }
            let sim = similarity(&docs[i], &docs[j], &idf);
            if sim >= SIM_THRESHOLD {
                uf.union(i, j);
                sims.insert((i, j), sim);
            }
        }
    }

    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        groups.entry(uf.find(i)).or_default().push(i);
    }

    let mut out = Vec::new();
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let mut member_keys: Vec<String> = members.iter().map(|&i| docs[i].key.clone()).collect();
        member_keys.sort();

        // Average pairwise similarity over members (rounded for snapshot stability).
        let mut total = 0.0;
        let mut count = 0;
        for a in 0..members.len() {
            for b in (a + 1)..members.len() {
                let (i, j) = (members[a].min(members[b]), members[a].max(members[b]));
                let sim = *sims
                    .get(&(i, j))
                    .unwrap_or(&similarity(&docs[i], &docs[j], &idf));
                total += sim;
                count += 1;
            }
        }
        let score = round2(if count > 0 { total / count as f64 } else { 0.0 });

        out.push(OverlapCluster {
            id: cluster_id(&member_keys),
            kind: ClusterKind::Similarity,
            reason: shared_terms_reason(members, &docs),
            members: member_keys,
            score: Some(score),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(t))
        .map(|t| t.to_string())
        .collect()
}

fn compute_idf(docs: &[Doc]) -> BTreeMap<String, f64> {
    let n = docs.len() as f64;
    let mut df: BTreeMap<String, usize> = BTreeMap::new();
    for d in docs {
        for term in &d.set {
            *df.entry(term.clone()).or_default() += 1;
        }
    }
    df.into_iter()
        .map(|(term, c)| (term, (n / c as f64).ln() + 1.0))
        .collect()
}

/// Blend of token-set Jaccard and TF-IDF cosine (research.md §3).
fn similarity(a: &Doc, b: &Doc, idf: &BTreeMap<String, f64>) -> f64 {
    let inter = a.set.intersection(&b.set).count() as f64;
    let union = a.set.union(&b.set).count() as f64;
    let jaccard = if union > 0.0 { inter / union } else { 0.0 };
    let cosine = tfidf_cosine(a, b, idf);
    round2(0.5 * jaccard + 0.5 * cosine)
}

fn tfidf_vec(doc: &Doc, idf: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
    let mut tf: BTreeMap<String, f64> = BTreeMap::new();
    for t in &doc.tokens {
        *tf.entry(t.clone()).or_default() += 1.0;
    }
    tf.into_iter()
        .map(|(t, c)| {
            let w = idf.get(&t).copied().unwrap_or(1.0);
            (t, c * w)
        })
        .collect()
}

fn tfidf_cosine(a: &Doc, b: &Doc, idf: &BTreeMap<String, f64>) -> f64 {
    let va = tfidf_vec(a, idf);
    let vb = tfidf_vec(b, idf);
    let mut dot = 0.0;
    for (t, wa) in &va {
        if let Some(wb) = vb.get(t) {
            dot += wa * wb;
        }
    }
    let na: f64 = va.values().map(|w| w * w).sum::<f64>().sqrt();
    let nb: f64 = vb.values().map(|w| w * w).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

fn shared_terms_reason(members: &[usize], docs: &[Doc]) -> String {
    // Terms common to every member, ranked by total frequency, top few.
    let mut common: Option<BTreeSet<String>> = None;
    for &i in members {
        let set = docs[i].set.clone();
        common = Some(match common {
            Some(c) => c.intersection(&set).cloned().collect(),
            None => set,
        });
    }
    let common = common.unwrap_or_default();
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for &i in members {
        for t in &docs[i].tokens {
            if common.contains(t) {
                *freq.entry(t.clone()).or_default() += 1;
            }
        }
    }
    let mut terms: Vec<(String, usize)> = freq.into_iter().collect();
    // Highest frequency first; alphabetical tiebreak for determinism.
    terms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top: Vec<String> = terms.into_iter().take(4).map(|(t, _)| t).collect();
    if top.is_empty() {
        "lexically similar name + description".to_string()
    } else {
        format!("shared terms: {}", top.join(", "))
    }
}

// ---- helpers ---------------------------------------------------------------------------

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Deterministic `c-<hex8>` id from the sorted member keys.
fn cluster_id(member_keys: &[String]) -> String {
    let mut hasher = Sha256::new();
    for k in member_keys {
        hasher.update(k.as_bytes());
        hasher.update([0u8]);
    }
    let hex = format!("{:x}", hasher.finalize());
    format!("c-{}", &hex[..8])
}

#[derive(Clone)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn connected(&mut self, a: usize, b: usize) -> bool {
        self.find(a) == self.find(b)
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r {
            r = self.parent[r];
        }
        // path compression
        let mut c = x;
        while self.parent[c] != r {
            let next = self.parent[c];
            self.parent[c] = r;
            c = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Attach higher index under lower for stable roots.
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.parent[hi] = lo;
        }
    }
}
