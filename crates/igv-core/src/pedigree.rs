//! PED pedigree file parser, trio ordering, and Mendelian violation checking.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::io::{fetch_text_maybe_gz, Location};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sex {
    Male,
    Female,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phenotype {
    Unaffected,
    Affected,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplePedigree {
    pub family_id: String,
    pub sample_id: String,
    pub father_id: Option<String>,
    pub mother_id: Option<String>,
    pub sex: Sex,
    pub phenotype: Phenotype,
}

#[derive(Debug, Clone, Default)]
pub struct Pedigree {
    samples: HashMap<String, SamplePedigree>,
    order: Vec<String>,
}

impl Pedigree {
    pub async fn open(spec: &str, auth_token: Option<&str>) -> Result<Self> {
        let loc = Location::parse(spec);
        let text = fetch_text_maybe_gz(&loc, auth_token).await?;
        Ok(Self::parse(&text))
    }

    pub async fn open_path(path: &Path) -> Result<Self> {
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| crate::error::IgvError::io(path, e))?;
        Ok(Self::parse(&text))
    }

    /// Parse standard 6-column whitespace-delimited PED format:
    /// `family_id  sample_id  father_id  mother_id  sex  phenotype`
    pub fn parse(text: &str) -> Self {
        let mut ped = Self::default();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = trimmed.split_whitespace().collect();
            if cols.len() < 4 {
                continue;
            }
            let family_id = cols[0].to_string();
            let sample_id = cols[1].to_string();
            let father_id = normalize_parent(cols[2]);
            let mother_id = normalize_parent(cols[3]);
            let sex = match cols.get(4).copied() {
                Some("1") => Sex::Male,
                Some("2") => Sex::Female,
                _ => Sex::Unknown,
            };
            let phenotype = match cols.get(5).copied() {
                Some("1") => Phenotype::Unaffected,
                Some("2") => Phenotype::Affected,
                _ => Phenotype::Unknown,
            };
            ped.order.push(sample_id.clone());
            ped.samples.insert(
                sample_id.clone(),
                SamplePedigree {
                    family_id,
                    sample_id,
                    father_id,
                    mother_id,
                    sex,
                    phenotype,
                },
            );
        }
        ped
    }

    pub fn get(&self, sample_id: &str) -> Option<&SamplePedigree> {
        self.samples.get(sample_id)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Order VCF sample indices into trio blocks (`Child`, `Mother`, `Father`),
    /// preserving original order for unrelated samples.
    pub fn order_samples(&self, vcf_samples: &[String]) -> Vec<usize> {
        if self.samples.is_empty() || vcf_samples.is_empty() {
            return (0..vcf_samples.len()).collect();
        }
        let pos_map: HashMap<&str, usize> = vcf_samples
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let mut used = vec![false; vcf_samples.len()];
        let mut out = Vec::with_capacity(vcf_samples.len());

        // Emit trios (children with at least one known parent in VCF) first.
        for sid in &self.order {
            let Some(&c_idx) = pos_map.get(sid.as_str()) else {
                continue;
            };
            let Some(rec) = self.samples.get(sid) else {
                continue;
            };
            if rec.father_id.is_some() || rec.mother_id.is_some() {
                if !used[c_idx] {
                    out.push(c_idx);
                    used[c_idx] = true;
                }
                if let Some(m) = &rec.mother_id {
                    if let Some(&m_idx) = pos_map.get(m.as_str()) {
                        if !used[m_idx] {
                            out.push(m_idx);
                            used[m_idx] = true;
                        }
                    }
                }
                if let Some(f) = &rec.father_id {
                    if let Some(&f_idx) = pos_map.get(f.as_str()) {
                        if !used[f_idx] {
                            out.push(f_idx);
                            used[f_idx] = true;
                        }
                    }
                }
            }
        }
        for (i, &u) in used.iter().enumerate() {
            if !u {
                out.push(i);
            }
        }
        out
    }

    /// Check whether a diploid child genotype `child` is Mendelian-consistent with
    /// diploid parent genotypes `father` and `mother`.
    pub fn is_mendelian_violation(
        child: &[Option<usize>],
        father: &[Option<usize>],
        mother: &[Option<usize>],
    ) -> bool {
        if child.len() != 2 || father.len() != 2 || mother.len() != 2 {
            return false;
        }
        let (Some(c0), Some(c1)) = (child[0], child[1]) else {
            return false;
        };
        let (Some(f0), Some(f1)) = (father[0], father[1]) else {
            return false;
        };
        let (Some(m0), Some(m1)) = (mother[0], mother[1]) else {
            return false;
        };
        let f_has_0 = f0 == c0 || f1 == c0;
        let m_has_1 = m0 == c1 || m1 == c1;
        let f_has_1 = f0 == c1 || f1 == c1;
        let m_has_0 = m0 == c0 || m1 == c0;
        !((f_has_0 && m_has_1) || (f_has_1 && m_has_0))
    }
}

fn normalize_parent(raw: &str) -> Option<String> {
    match raw.trim() {
        "" | "0" | "." | "-" => None,
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mendelian_violations_in_trios() {
        // 0/0 x 0/0 -> 0/1 is violation
        assert!(Pedigree::is_mendelian_violation(
            &[Some(0), Some(1)],
            &[Some(0), Some(0)],
            &[Some(0), Some(0)],
        ));
        // 0/0 x 1/1 -> 0/1 is valid
        assert!(!Pedigree::is_mendelian_violation(
            &[Some(0), Some(1)],
            &[Some(0), Some(0)],
            &[Some(1), Some(1)],
        ));
        // 1/1 x 1/1 -> 0/1 is violation
        assert!(Pedigree::is_mendelian_violation(
            &[Some(0), Some(1)],
            &[Some(1), Some(1)],
            &[Some(1), Some(1)],
        ));
    }

    #[test]
    fn parses_ped_and_orders_trios() {
        let ped = Pedigree::parse("FAM1\tfather\t0\t0\t1\t1\nFAM1\tmother\t0\t0\t2\t1\nFAM1\tchild\tfather\tmother\t1\t2\n");
        let vcf = vec!["unrelated".into(), "father".into(), "child".into(), "mother".into()];
        let order = ped.order_samples(&vcf);
        let ordered: Vec<&str> = order.iter().map(|&i| vcf[i].as_str()).collect();
        assert_eq!(ordered, vec!["child", "mother", "father", "unrelated"]);
    }
}
