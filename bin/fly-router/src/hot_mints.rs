use crate::debug_tools;
use router_config_lib::HotMintsConfig;
use solana_program::pubkey::Pubkey;
use std::collections::{HashSet, VecDeque};
use std::str::FromStr;
use tracing::debug;

//一个简单的缓存机制，用于管理热门铸币公钥，并提供了相应的测试来验证其功能

pub struct HotMintsCache {
    max_count: usize,//表示缓存中最多能存储的非 “始终热门” 铸币公钥的数量。
    always_hot: HashSet<Pubkey>,//一个 HashSet，存储那些始终被认为是热门的铸币公钥
    latest_unordered: HashSet<Pubkey>,//一个 HashSet，存储最近添加的非 “始终热门” 铸币公钥，用于快速查找
    latest_ordered: VecDeque<Pubkey>,//一个 VecDeque，存储最近添加的非 “始终热门” 铸币公钥，按添加顺序排列，用于管理缓存大小
}

impl HotMintsCache {
    pub fn new(config: &Option<HotMintsConfig>) -> Self {
        let config = config.clone().unwrap_or(HotMintsConfig {
            always_hot_mints: vec![
                "So11111111111111111111111111111111111111112".to_string(),
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
            ],
            keep_latest_count: 100,
        });

        HotMintsCache {
            max_count: config.keep_latest_count,
            always_hot: config
                .always_hot_mints
                .iter()
                .map(|x| Pubkey::from_str(x).unwrap())
                .collect(),
            latest_unordered: Default::default(),
            latest_ordered: Default::default(),
        }
    }

    pub fn add(&mut self, pubkey: Pubkey) {
        if self.always_hot.contains(&pubkey) {
            return;
        }

        if self.latest_unordered.contains(&pubkey) {
            let position = self
                .latest_ordered
                .iter()
                .position(|x| *x == pubkey)
                .unwrap();
            self.latest_ordered.remove(position);
        } else if self.latest_unordered.len() >= self.max_count {
            let oldest = self.latest_ordered.pop_back().unwrap();
            self.latest_unordered.remove(&oldest);
            debug!("Removing {} from hot mints", debug_tools::name(&oldest));
        }

        if self.latest_unordered.insert(pubkey) {
            debug!("Adding {} to hot mints", debug_tools::name(&pubkey));
        }
        self.latest_ordered.push_front(pubkey);
        return;
    }

    pub fn get(&self) -> HashSet<Pubkey> {
        self.latest_unordered
            .union(&self.always_hot)
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::hot_mints::HotMintsCache;
    use itertools::Itertools;
    use router_config_lib::HotMintsConfig;
    use solana_program::pubkey::Pubkey;
    use std::collections::HashSet;
    use std::str::FromStr;

    #[test]
    pub fn should_keep_hottest_in_list() {
        let jito = Pubkey::from_str("J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn").unwrap();
        let tokens = (0..10).map(|_| Pubkey::new_unique()).collect_vec();

        let mut cache = HotMintsCache::new(&Some(HotMintsConfig {
            always_hot_mints: vec![jito.to_string()],
            keep_latest_count: 3,
        }));

        assert_eq!(cache.get().len(), 1);
        assert_eq!(cache.get(), HashSet::from([jito]));

        cache.add(tokens[0]);
        cache.add(tokens[1]);
        cache.add(tokens[2]);
        cache.add(tokens[1]);
        cache.add(tokens[1]);
        cache.add(tokens[2]);

        assert_eq!(cache.get().len(), 4);
        assert_eq!(
            cache.get(),
            HashSet::from([jito, tokens[0], tokens[1], tokens[2]])
        );

        cache.add(tokens[3]);

        assert_eq!(cache.get().len(), 4);
        assert_eq!(
            cache.get(),
            HashSet::from([jito, tokens[1], tokens[2], tokens[3]])
        );

        cache.add(jito);

        assert_eq!(cache.get().len(), 4);
        assert_eq!(
            cache.get(),
            HashSet::from([jito, tokens[1], tokens[2], tokens[3]])
        );
    }
}
