use std::collections::VecDeque;

use crate::terrain::{CAVE, CAVE_WALL, DEEP_SEA, NO_START, SEA, VOID_LAND};

pub type Node = (usize, u32);
pub type Link = (Node, Node);

pub struct PlaneGraph {
    pub flags: Vec<u64>,
    pub nbors: Vec<Vec<u32>>,
    pub areas: Vec<u32>,
    pub cave: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    pub nation: u32,
    pub uw: bool,
    pub coast: bool,
    pub cave: u8,
    pub likesterr: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub nation: u32,
    pub plane: usize,
    pub prov: u32,
}

const FAR: u32 = 8;

struct Graph {
    offsets: Vec<usize>,
    adj: Vec<Vec<usize>>,
    flags: Vec<u64>,
    cave: Vec<bool>,
    usable: Vec<bool>,
    coastal: Vec<bool>,
}

impl Graph {
    fn build(planes: &[PlaneGraph], gates: &[Link]) -> Graph {
        let mut offsets = Vec::with_capacity(planes.len() + 1);
        let mut total = 0;
        for p in planes {
            offsets.push(total);
            total += p.flags.len();
        }
        offsets.push(total);
        let mut adj = vec![Vec::new(); total];
        let mut flags = vec![0u64; total];
        let mut cave = vec![false; total];
        let mut usable = vec![false; total];
        let mut coastal = vec![false; total];
        for (pi, p) in planes.iter().enumerate() {
            for prov in 1..p.flags.len() {
                let node = offsets[pi] + prov;
                let f = p.flags[prov];
                flags[node] = f;
                cave[node] = p.cave || f & CAVE != 0;
                let area = p.areas.get(prov).copied().unwrap_or(1);
                usable[node] = area > 0
                    && f & (NO_START | CAVE_WALL | VOID_LAND) == 0
                    && p.nbors.get(prov).map(|n| !n.is_empty()).unwrap_or(false);
                if let Some(ns) = p.nbors.get(prov) {
                    for &n in ns {
                        let n = n as usize;
                        if n > 0 && n < p.flags.len() {
                            adj[node].push(offsets[pi] + n);
                            if f & SEA == 0 && p.flags[n] & SEA != 0 {
                                coastal[node] = true;
                            }
                        }
                    }
                }
            }
        }
        for ((pa, a), (pb, b)) in gates {
            if *pa < planes.len() && *pb < planes.len() {
                let na = offsets[*pa] + *a as usize;
                let nb = offsets[*pb] + *b as usize;
                if na < total && nb < total {
                    adj[na].push(nb);
                    adj[nb].push(na);
                }
            }
        }
        Graph {
            offsets,
            adj,
            flags,
            cave,
            usable,
            coastal,
        }
    }

    fn locate(&self, node: usize) -> (usize, u32) {
        let plane = self
            .offsets
            .iter()
            .rposition(|&o| o <= node)
            .unwrap_or(0)
            .min(self.offsets.len() - 2);
        (plane, (node - self.offsets[plane]) as u32)
    }

    fn distances(&self, sources: &[usize]) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.adj.len()];
        let mut queue = VecDeque::new();
        for &s in sources {
            dist[s] = 0;
            queue.push_back(s);
        }
        while let Some(n) = queue.pop_front() {
            let d = dist[n] + 1;
            for &m in &self.adj[n] {
                if dist[m] == u32::MAX {
                    dist[m] = d;
                    queue.push_back(m);
                }
            }
        }
        dist
    }
}

fn fits(g: &Graph, node: usize, want: &Want, relax: u8) -> bool {
    if !g.usable[node] {
        return false;
    }
    let f = g.flags[node];
    let water = f & SEA != 0;
    if relax < 3 && water != want.uw {
        return false;
    }
    if relax < 2 {
        match want.cave {
            0 => {
                if g.cave[node] {
                    return false;
                }
            }
            2 | 3 => {
                if !g.cave[node] {
                    return false;
                }
            }
            _ => {}
        }
    }
    if relax < 1 && want.coast && !g.coastal[node] {
        return false;
    }
    true
}

fn tier(g: &Graph, node: usize, want: &Want) -> u32 {
    let f = g.flags[node];
    let mut t = 0;
    if want.likesterr != 0 && f & want.likesterr != 0 {
        t += 1;
    }
    if want.uw && want.likesterr & DEEP_SEA != 0 && f & DEEP_SEA != 0 {
        t += 1;
    }
    t
}

fn jitter(seed: u64, node: usize, round: u32) -> f32 {
    let mut x = seed ^ (node as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ ((round as u64) << 40);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x & 0xffff) as f32 / 65536.0
}

fn rank(want: &Want) -> u32 {
    let mut r = 0;
    if want.uw {
        r += 8;
    }
    if want.cave >= 2 {
        r += 4;
    }
    if want.coast {
        r += 2;
    }
    if want.likesterr != 0 {
        r += 1;
    }
    r
}

fn best(g: &Graph, want: &Want, taken: &[usize], seed: u64, round: u32) -> Option<usize> {
    let dist = g.distances(taken);
    for relax in 0..4u8 {
        let mut top: Option<(f32, usize)> = None;
        for (node, &dn) in dist.iter().enumerate() {
            if taken.contains(&node) || !fits(g, node, want, relax) {
                continue;
            }
            let d = if taken.is_empty() { FAR } else { dn.min(FAR) };
            if d == 0 {
                continue;
            }
            let score =
                d as f32 * 4.0 + tier(g, node, want) as f32 * 6.0 + jitter(seed, node, round);
            if top.map(|(s, _)| score > s).unwrap_or(true) {
                top = Some((score, node));
            }
        }
        if let Some((_, node)) = top {
            return Some(node);
        }
    }
    None
}

fn min_pairwise(g: &Graph, nodes: &[usize]) -> u32 {
    let mut m = u32::MAX;
    for (i, &a) in nodes.iter().enumerate() {
        let dist = g.distances(&[a]);
        for &b in &nodes[i + 1..] {
            m = m.min(dist[b]);
        }
    }
    m
}

pub fn place(planes: &[PlaneGraph], gates: &[Link], wants: &[Want], seed: u64) -> Vec<Placed> {
    let g = Graph::build(planes, gates);
    let mut order: Vec<usize> = (0..wants.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(rank(&wants[i])));
    let mut chosen: Vec<Option<usize>> = vec![None; wants.len()];
    let mut taken: Vec<usize> = Vec::new();
    for &i in &order {
        if let Some(node) = best(&g, &wants[i], &taken, seed, 0) {
            chosen[i] = Some(node);
            taken.push(node);
        }
    }
    for round in 1..=3u32 {
        let mut improved = false;
        for &i in &order {
            let Some(current) = chosen[i] else {
                continue;
            };
            let others: Vec<usize> = chosen
                .iter()
                .enumerate()
                .filter(|(j, c)| *j != i && c.is_some())
                .map(|(_, c)| c.unwrap())
                .collect();
            let before = {
                let mut all = others.clone();
                all.push(current);
                min_pairwise(&g, &all)
            };
            if let Some(node) = best(&g, &wants[i], &others, seed, round) {
                if node != current {
                    let mut all = others.clone();
                    all.push(node);
                    let after = min_pairwise(&g, &all);
                    if after > before {
                        chosen[i] = Some(node);
                        improved = true;
                    }
                }
            }
        }
        if !improved {
            break;
        }
    }
    wants
        .iter()
        .zip(&chosen)
        .filter_map(|(w, c)| {
            c.map(|node| {
                let (plane, prov) = g.locate(node);
                Placed {
                    nation: w.nation,
                    plane,
                    prov,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::FOREST;

    fn grid(w: usize, h: usize, sea: &[usize], forest: &[usize]) -> PlaneGraph {
        let n = w * h;
        let mut flags = vec![0u64; n + 1];
        let mut nbors = vec![Vec::new(); n + 1];
        for i in 0..n {
            let (x, y) = (i % w, i / w);
            let p = i + 1;
            if sea.contains(&p) {
                flags[p] |= SEA;
            }
            if forest.contains(&p) {
                flags[p] |= FOREST;
            }
            if x + 1 < w {
                nbors[p].push((p + 1) as u32);
                nbors[p + 1].push(p as u32);
            }
            if y + 1 < h {
                nbors[p].push((p + w) as u32);
                nbors[p + w].push(p as u32);
            }
        }
        PlaneGraph {
            flags,
            nbors,
            areas: vec![1; n + 1],
            cave: false,
        }
    }

    fn want(nation: u32) -> Want {
        Want {
            nation,
            uw: false,
            coast: false,
            cave: 0,
            likesterr: 0,
        }
    }

    #[test]
    fn water_nations_get_sea_and_land_nations_get_land_far_apart() {
        let plane = grid(6, 6, &[1, 2, 7, 8, 36, 35], &[15, 16]);
        let wants = vec![
            Want {
                uw: true,
                ..want(43)
            },
            want(5),
            Want {
                likesterr: FOREST,
                ..want(7)
            },
            Want {
                coast: true,
                ..want(29)
            },
        ];
        let placed = place(&[plane], &[], &wants, 7);
        assert_eq!(placed.len(), 4);
        let plane = grid(6, 6, &[1, 2, 7, 8, 36, 35], &[15, 16]);
        let by = |n: u32| placed.iter().find(|p| p.nation == n).unwrap().prov as usize;
        assert!(plane.flags[by(43)] & SEA != 0);
        assert!(plane.flags[by(5)] & SEA == 0);
        let alone = place(
            &[grid(6, 6, &[1, 2, 7, 8, 36, 35], &[15, 16])],
            &[],
            &[Want {
                likesterr: FOREST,
                ..want(7)
            }],
            7,
        );
        assert!(plane.flags[alone[0].prov as usize] & FOREST != 0);
        let coast = by(29);
        assert!(plane.flags[coast] & SEA == 0);
        assert!(plane.nbors[coast]
            .iter()
            .any(|&n| plane.flags[n as usize] & SEA != 0));
        let mut provs: Vec<u32> = placed.iter().map(|p| p.prov).collect();
        provs.dedup();
        assert_eq!(provs.len(), 4);
        for a in &placed {
            for b in &placed {
                if a.nation != b.nation {
                    let (ax, ay) = ((a.prov as usize - 1) % 6, (a.prov as usize - 1) / 6);
                    let (bx, by2) = ((b.prov as usize - 1) % 6, (b.prov as usize - 1) / 6);
                    assert!(ax.abs_diff(bx) + ay.abs_diff(by2) >= 2);
                }
            }
        }
    }

    #[test]
    fn cave_nations_go_to_the_cave_plane_through_the_gates() {
        let surface = grid(4, 4, &[], &[]);
        let mut caves = grid(3, 3, &[], &[]);
        caves.cave = true;
        let wants = vec![
            Want {
                cave: 2,
                ..want(15)
            },
            want(5),
            want(6),
        ];
        let placed = place(&[surface, caves], &[((0, 1), (1, 1))], &wants, 3);
        assert_eq!(placed.len(), 3);
        let agartha = placed.iter().find(|p| p.nation == 15).unwrap();
        assert_eq!(agartha.plane, 1);
        assert!(placed
            .iter()
            .filter(|p| p.nation != 15)
            .all(|p| p.plane == 0));
    }

    #[test]
    fn no_start_provinces_are_skipped() {
        let mut plane = grid(3, 1, &[], &[]);
        plane.flags[1] |= NO_START;
        plane.flags[3] |= NO_START;
        let placed = place(&[plane], &[], &[want(5)], 1);
        assert_eq!(placed[0].prov, 2);
    }
}
