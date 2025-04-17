use std::sync::Arc;

// 引入自定义的预导入模块，包含常用的类型和特性
use crate::prelude::*;

//#[derive(Debug, Clone, PartialEq, Eq, Hash)]
// pub struct Edge {
//     /// 边唯一标识，和 input_mint 与 output_mint 组合唯一
//     /// 例如：`"unique_id:input_mint:output_mint"`
//     pub unique_id: String,
//     /// 输入代币的铸币地址
//     pub input_mint: Pubkey,
//     /// 输出代币的铸币地址
//     pub output_mint: Pubkey,
//     /// 是否为双向边
//     pub bidirectional: bool,
// }

pub struct Graph {
    /// 存储所有边的映射表 ((poolkey, input_mint) → EdgeWrapper)
    edges: HashMap<(Pubkey, Pubkey), Arc<Edge>>,
    /// 邻接表 (节点 → Vec<(poolkey, output_mint)>)
    adjacency: HashMap<Pubkey, Vec<(Pubkey, Pubkey)>>,
}

impl Graph {
    /// 创建一个空图
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
            adjacency: HashMap::new(),
        }
    }

    /// 向图中添加一条边
    pub fn add_edges(&mut self, edges: Vec<Arc<Edge>>) {
        for edge in edges {
            self.add_edge(edge);
        }
    }


    /// 向图中添加一条边
    pub fn add_edge(&mut self, edge: Arc<Edge>) {
        // 检查边是否已经存在
        if self.edges.contains_key(&(edge.key(), edge.input_mint)) {
            // 如果已经存在，则不添加
            return;
        }
        let key = edge.unique_id();
        // 插入到边映射
        self.edges.insert(key, Arc::clone(&edge));
        // 在邻接表中添加单向连接
        self.adjacency
            .entry(edge.input_mint)
            .or_default()
            .push((edge.key(), edge.output_mint));
        
        // 如果是双向边，则也要添加反向的连接
        // if edge.bidirectional {
        //     // 反向 key
        //     let rev_key = (edge.unique_id.clone(), edge.output_mint);
        //     self.edges.insert(rev_key.clone(), Edge {
        //         unique_id: edge.unique_id.clone(),
        //         input_mint: edge.output_mint,
        //         output_mint: edge.input_mint,
        //         bidirectional: true,
        //     });
        //     self.adjacency
        //         .entry(edge.output_mint)
        //         .or_default()
        //         .push((edge.unique_id.clone(), edge.input_mint));
        // }
    }

    /// 寻找从 start 出发，经过不超过 max_hops 条边后回到 start 的所有路径
    /// 要求：路径中不能重复使用同一条边（key），且不包含自循环（长度为1的环）
    pub fn find_cycles(
        &self,
        start: Pubkey,
        max_hops: usize,
    ) -> Vec<Vec<Arc<Edge>>> {
        let mut results = Vec::new();
        let mut path = Vec::new();
        let mut used_edges = HashSet::new();

        // 内部递归 DFS
        fn dfs(
            graph: &Graph,
            start: Pubkey,
            current: Pubkey,
            max_hops: usize,
            path: &mut Vec<Arc<Edge>>,
            used_edges: &mut HashSet<Pubkey>,
            results: &mut Vec<Vec<Arc<Edge>>>,
        ) {
            if path.len() > max_hops {
                return;
            }
            if current == start && !path.is_empty() {
                // 找到一条从 start 回到 start 的路径
                results.push(path.clone());
                // 继续搜索，可能还有更长的环
            }
            if path.len() == max_hops {
                return;
            }
            if let Some(neighbors) = graph.adjacency.get(&current) {
                for (eid, next) in neighbors {
                    if used_edges.contains(eid) {
                        continue;
                    }
                    // 取出对应的 EdgeWrapper
                    let edge = graph
                        .edges
                        .get(&(eid.clone(), current))
                        .expect("edge must exist")
                        .clone();
                    // 标记
                    used_edges.insert(eid.clone());
                    path.push(edge.clone());

                    dfs(graph, start, *next, max_hops, path, used_edges, results);

                    // 回溯
                    path.pop();
                    used_edges.remove(eid);
                }
            }
        }

        dfs(
            self,
            start,
            start,
            max_hops,
            &mut path,
            &mut used_edges,
            &mut results,
        );

        // 过滤掉“自循环”：长度为1且 input_mint == output_mint 的路径
        results
            .into_iter()
            .filter(|cycle| {
                !(cycle.len() == 1 && cycle[0].input_mint == cycle[0].output_mint)
            })
            .collect()
    }
}



#[cfg(test)]
mod tests {
    use router_lib::chain_data::ChainDataArcRw;
    use router_lib::dex::{AccountProviderView, ChainDataAccountProvider, DexInterface};

    use crate::graph::Graph;
    use crate::mock::test::{MockDexIdentifier, MockDexInterface};
    use crate::prelude::*;

    fn pubkey_from_u8(x: u8) -> Pubkey {
        let mut bytes = [0u8; 32];
        bytes[0] = x;
        Pubkey::new_from_array(bytes)
    }

    #[test]
    fn test_add_and_lookup_edges() {
        let mut g = Graph::new();
        let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
            Default::default(),
        ))) as AccountProviderView;
        let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

        let usdc = pubkey_from_u8(1);
        let sol = pubkey_from_u8(2);
        let pool_1 = Pubkey::new_unique();

        let e1 = Arc::new(make_edge(
            &dex,
            &pool_1,
            &usdc,
            &sol,
            &chain_data,
            6,
            1.0,
            1.0 / 0.1495,
        ));
        let e2 = Arc::new(make_edge(
            &dex,
            &pool_1,
            &sol,
            &usdc,
            &chain_data,
            6,
            1.0,
            0.1495,
        ));
        
        g.add_edge(e1.clone());
        g.add_edge(e2.clone());

        // 验证边的添加
        assert_eq!(g.edges.get(&(pool_1, usdc)).unwrap().unique_id(), e1.unique_id());
        assert!(g.adjacency.get(&usdc).unwrap().contains(&(pool_1, sol)));
        assert!(g.adjacency.get(&sol).unwrap().contains(&(pool_1, usdc)));
    }

    #[test]
    fn test_bidirectional_edge() {
        let mut g = Graph::new();
        let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
            Default::default(),
        ))) as AccountProviderView;
        let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

        let a = pubkey_from_u8(1);
        let b = pubkey_from_u8(2);
        let pool = Pubkey::new_unique();

        let edge = Arc::new(make_edge(
            &dex,
            &pool,
            &a,
            &b,
            &chain_data,
            6,
            1.0,
            1.0,
        ));
        
        g.add_edge(edge.clone());

        // 验证边的添加
        assert!(g.adjacency.get(&a).unwrap().contains(&(pool, b)));
        // 由于我们修改了双向边的实现，这里应该是 None
        assert!(g.adjacency.get(&b).is_none());
    }

    #[test]
    fn test_find_simple_cycle() {
        let mut g = Graph::new();
        let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
            Default::default(),
        ))) as AccountProviderView;
        let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

        let a = pubkey_from_u8(1);
        let b = pubkey_from_u8(2);
        let pool = Pubkey::new_unique();

        let edge = Arc::new(make_edge(
            &dex,
            &pool,
            &a,
            &b,
            &chain_data,
            6,
            1.0,
            1.0,
        ));
        
        g.add_edge(edge);

        let cycles = g.find_cycles(a, 2);
        // 由于没有形成环，应该返回空
        assert_eq!(cycles.len(), 0);
    }

    #[test]
    fn test_find_multiple_edges_between_same_nodes() {
        let mut g = Graph::new();
        let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
            Default::default(),
        ))) as AccountProviderView;
        let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

        let a = pubkey_from_u8(1);
        let b = pubkey_from_u8(2);

        // 添加多个边
        for _ in 0..3 {
            let pool = Pubkey::new_unique();
     //       let pool_backward = Pubkey::new_unique();
            
            g.add_edge(Arc::new(make_edge(
                &dex,
                &pool,
                &a,
                &b,
                &chain_data,
                6,
                1.0,
                1.0,
            )));

            g.add_edge(Arc::new(make_edge(
                &dex,
                &pool,
                &b,
                &a,
                &chain_data,
                6,
                1.0,
                1.0,
            )));
        }

        let cycles = g.find_cycles(a, 2);
        assert_eq!(cycles.len(), 6); // 2 * 3 = 6 种可能的循环

        let mut seen = HashSet::new();
        for path in &cycles {
            assert_eq!(path.len(), 2); // 每个循环应该包含2条边
            assert!(seen.insert(format!("{:?}-{:?}", 
                path[0].id.key(),
                path[1].id.key()
            )));
        }
    }

   #[test]
   fn test_no_self_loop_cycles() {
       let mut g = Graph::new();
       let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
           Default::default(),
       ))) as AccountProviderView;
       let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

       let a = pubkey_from_u8(1);
       let pool = Pubkey::new_unique();

       // 创建一个自循环边
       let edge = Arc::new(make_edge(
           &dex,
           &pool,
           &a,
           &a,
           &chain_data,
           6,
           1.0,
           1.0,
       ));
       
       g.add_edge(edge);

       let cycles = g.find_cycles(a, 3);
       // 验证没有自循环
       assert!(cycles.is_empty());
   }

   #[test]
   fn test_three_node_triangle_with_multiple_edges() {
       let mut g = Graph::new();
       let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
           Default::default(),
       ))) as AccountProviderView;
       let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

       let a = pubkey_from_u8(1);
       let b = pubkey_from_u8(2);
       let c = pubkey_from_u8(3);

       // 添加 a->b 的多条边
       for _ in 0..3 {
           let pool = Pubkey::new_unique();
           g.add_edge(Arc::new(make_edge(
               &dex,
               &pool,
               &a,
               &b,
               &chain_data,
               6,
               1.0,
               1.0,
           )));

           g.add_edge(Arc::new(make_edge(
            &dex,
            &pool,
            &b,
            &a,
            &chain_data,
            6,
            1.0,
            1.0,
        )));
       }

       // 添加 b->c 的多条边
       for _ in 0..3 {
           let pool = Pubkey::new_unique();
           g.add_edge(Arc::new(make_edge(
               &dex,
               &pool,
               &b,
               &c,
               &chain_data,
               6,
               1.0,
               1.0,
           )));

           g.add_edge(Arc::new(make_edge(
            &dex,
            &pool,
            &c,
            &b,
            &chain_data,
            6,
            1.0,
            1.0,
        )));
       }

       // 添加 c->a 的多条边
       for _ in 0..3 {
           let pool = Pubkey::new_unique();
           g.add_edge(Arc::new(make_edge(
               &dex,
               &pool,
               &c,
               &a,
               &chain_data,
               6,
               1.0,
               1.0,
           )));

           g.add_edge(Arc::new(make_edge(
            &dex,
            &pool,
            &a,
            &c,
            &chain_data,
            6,
            1.0,
            1.0,
        )));
       }

       let cycles = g.find_cycles(a, 3);
       
       // 统计不同长度的环路
       let mut length_2_cycles = 0;
       let mut length_3_cycles = 0;
       
       for path in &cycles {
           match path.len() {
               2 => length_2_cycles += 1,
               3 => length_3_cycles += 1,
               _ => panic!("不应该出现其他长度的环路"),
           }
       }

       // 验证环路数量
       assert_eq!(length_2_cycles, 12); // 6 + 6 = 12 条边形成的环路
       assert_eq!(length_3_cycles, 54); // 3*9*2 = 54 条边形成的环路
       // 验证总环路数量
       assert_eq!(cycles.len(), 12+54); // 总数应该是 12+54 个
   }

   #[test]
   fn test_three_node_triangle_with_two_edges() {
       let mut g = Graph::new();
       let chain_data = Arc::new(ChainDataAccountProvider::new(ChainDataArcRw::new(
           Default::default(),
       ))) as AccountProviderView;
       let dex = Arc::new(MockDexInterface {}) as Arc<dyn DexInterface>;

       let a = pubkey_from_u8(1);
       let b = pubkey_from_u8(2);
       let c = pubkey_from_u8(3);

       // 添加 a->b 的边
       for _ in 0..3 {
           let pool = Pubkey::new_unique();
           g.add_edge(Arc::new(make_edge(
               &dex,
               &pool,
               &a,
               &b,
               &chain_data,
               6,
               1.0,
               1.0,
           )));

           g.add_edge(Arc::new(make_edge(
            &dex,
            &pool,
            &b,
            &a,
            &chain_data,
            6,
            1.0,
            1.0,
        )));
       }

       // 添加 c->a 的边
       for _ in 0..3 {
           let pool = Pubkey::new_unique();
           g.add_edge(Arc::new(make_edge(
               &dex,
               &pool,
               &c,
               &a,
               &chain_data,
               6,
               1.0,
               1.0,
           )));

           g.add_edge(Arc::new(make_edge(
            &dex,
            &pool,
            &a,
            &c,
            &chain_data,
            6,
            1.0,
            1.0,
        )));
       }

       let cycles = g.find_cycles(a, 3);


       // 统计不同长度的环路
       let mut length_2_cycles = 0;

       for path in &cycles {
            match path.len() {
                2 => length_2_cycles += 1,
                _ => panic!("不应该出现其他长度的环路"),
            }
        }
        assert_eq!(length_2_cycles, 6 + 6); // 6 + 6 = 12 条边形成的环路

       assert_eq!(cycles.len(), length_2_cycles); // 所有的环路都是长度为2的环路
   }

    fn make_edge(
        dex: &Arc<dyn DexInterface>,
        key: &Pubkey,
        input_mint: &Pubkey,
        output_mint: &Pubkey,
        chain_data: &AccountProviderView,
        decimals: u8,
        input_price_usd: f64,
        pool_price: f64,
    ) -> Edge {
        let edge = Edge {
            input_mint: input_mint.clone(),
            output_mint: output_mint.clone(),
            dex: dex.clone(),
            id: Arc::new(MockDexIdentifier {
                key: key.clone(),
                input_mint: input_mint.clone(),
                output_mint: output_mint.clone(),
                price: pool_price,
            }),
            accounts_needed: 10,
            state: Default::default(),
        };

        edge.update_internal(chain_data, decimals, input_price_usd, &vec![100, 1000]);
        edge
    }


}