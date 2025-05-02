use std::{
    sync::{Arc, RwLock},
    net::{SocketAddr, Ipv4Addr},
};
use get_if_addrs::{self, IfAddr};
use solana_client::client_error::reqwest::{self, Client, ClientBuilder};
use tracing::{info, error};
use anyhow::{Result, Context};


#[derive(Clone, Debug, Default, serde_derive::Serialize, serde_derive::Deserialize)]
pub struct State {
    current_index: usize,
}

// 结构体定义
#[derive(Debug)]
pub struct ClientProvider {
    clients: Vec<Arc<Client>>,
    ips: Vec<String>,
    pub state: Arc<RwLock<State>>,
}

impl ClientProvider {
    /// 创建新的客户端提供者实例
    pub fn new() -> Result<Self> {
        let ips = Self::get_system_ips()?;
        info!("Found {} local network interfaces", ips.len());

        let mut clients = Vec::with_capacity(ips.len());
        let mut ip_strs = Vec::with_capacity(ips.len());

        for ip in &ips {
            let local_ip = ip.parse::<Ipv4Addr>()
                .with_context(|| format!("Failed to parse IP address: {}", ip))?;
            
            let local_addr = SocketAddr::new(local_ip.into(), 0); // 0表示自动分配端口
            
            let client = ClientBuilder::new()
                .local_address(Some(local_addr.ip()))
                .timeout(std::time::Duration::from_secs(5)) // 添加超时设置
                .build()
                .with_context(|| format!("Failed to build client for IP: {}", ip))?;

            clients.push(Arc::new(client));
            ip_strs.push(ip.clone());
            
            info!("Created client for IP: {}", ip);
        }

        if clients.is_empty() {
            error!("No available network interfaces found");
            return Err(anyhow::anyhow!("No available network interfaces"));
        }
        let state = State {
            current_index: 0,
        };

        Ok(Self {
            clients,
            ips: ip_strs,
            state: Arc::new(RwLock::new(state)),
        })
    }

    // /// 获取下一个可用的客户端
    // pub fn get_next_client(&mut self) -> Arc<Client> {
    //     let client = self.clients[self.current_index].clone();
    //     self.current_index = (self.current_index + 1) % self.clients.len();
    //     client
    // }

    pub fn get_next_client_by_index(&self, index: usize) -> Arc<Client> {
        if index < self.clients.len() {
            self.clients[index].clone()
        } else {
            panic!("Index out of bounds")
        }
    }

    pub fn get_next_clinet_index(&self) -> usize {
        let mut state = self.state.write().unwrap();
        let index = state.current_index;
        state.current_index = (state.current_index + 1) % self.clients.len();
        index
    }

    /// 获取当前使用的IP地址
    pub fn get_current_ip(&self) -> &str {
        let state = self.state.read().unwrap();
        &self.ips[state.current_index]
    }

    /// 获取所有可用的客户端数量
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// 获取系统所有可用的IPv4地址
    fn get_system_ips() -> Result<Vec<String>> {
        let mut ips = Vec::new();
        
        let interfaces = get_if_addrs::get_if_addrs()
            .context("Failed to get network interfaces")?;

        for interface in interfaces {
            if let IfAddr::V4(addr) = interface.addr {
                // 排除回环地址
                if !addr.ip.is_loopback() {
                    ips.push(addr.ip.to_string());
                }
            }
        }

        Ok(ips)
    }
}

impl Default for ClientProvider {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            error!("Failed to create default ClientProvider: {}", e);
            panic!("Could not create default ClientProvider");
        })
    }
}
