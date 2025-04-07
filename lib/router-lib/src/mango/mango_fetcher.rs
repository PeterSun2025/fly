use anyhow::Context;
use router_config_lib::Config;
use serde_derive::{Deserialize, Serialize};
use solana_client::client_error::reqwest;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/*
交易组基本信息：每个组都有一个公钥，像是一个独一无二的身份标识，还有所属的集群（分为主网和测试网等）、组名。同时，还给出了与之关联的 Mango 程序 ID、保险库地址、保险铸币地址和保险铸币的小数位数。
永续市场信息：部分交易组包含永续市场的相关数据，例如每个永续市场都有公钥、市场名称（像 “BTC-PERP”“SOL-PERP” 等）、基础资产的小数位数、基础资产和报价资产的最小交易数量、预言机地址，以及市场是否处于活跃状态和结算代币的索引。
代币信息：列出了大量代币的详细情况，每个代币都有自己的铸币地址、在组内的索引、交易符号（如 “JUP”“SOL”“USDC” 等）、小数位数、预言机地址、铸币信息以及与之相关的银行信息和是否活跃的状态。
其他市场信息：还提到了血清 3 市场（Serum3 Markets）和未列出具体内容的 Openbook V2 市场。血清 3 市场包含市场公钥、市场名称（如 “SOL/USDC”“ETH/USDC” 等）、基础代币和报价代币的索引、血清程序地址和外部市场地址，以及市场的活跃状态 。此外，还有存根预言机（Stub Oracles）相关信息，记录了特定铸币和其对应的预言机公钥。
*/
#[derive(Clone)]
pub struct MangoMetadata {
    pub mints: HashSet<Pubkey>,
    pub obv2_markets: HashSet<Pubkey>,
}

/*
{
			"publicKey": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
			"cluster": "MAINNET",
			"name": "MAINNET.0",
			"mangoProgramId": "4MangoMjqJ2firMokCjjGgoK8d4MXcrgL7XJaL3w6fVg",
			"insuranceVault": "F1vqFqkZHh5jd5rf8BcEzT8kqfd1snB1adRkayJ9KPNY",
			"insuranceMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
			"insuranceMintDecimals": 6,
			"perpMarkets": [
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "HwhVGkfsSQ9JSQeQYu2CbkRCLvsh3qRZxG6m4oMVwZpN",
					"marketIndex": 0,
					"name": "BTC-PERP",
					"baseDecimals": 6,
					"baseLotSize": 100,
					"quoteLotSize": 10,
					"oracle": "GVXRSBjFk6e6J3NbVPXohDJetcTjaeeuykUpbQF8UoMU",
					"active": true,
					"settleTokenIndex": 0
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "GcMimCLCU8aQhUpZwB5dWTQDxkTzuMy8uKQfujjYjz4b",
					"marketIndex": 4,
					"name": "RENDER-PERP",
					"baseDecimals": 6,
					"baseLotSize": 100000,
					"quoteLotSize": 10,
					"oracle": "CYGfrBJB9HgLf9iZyN4aH5HvUAi2htQ4MjPxeXMf4Egn",
					"active": true,
					"settleTokenIndex": 0
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9Y8paZ5wUpzLFfQuHz8j2RtPrKsDtHx9sbgFmWb5abCw",
					"marketIndex": 1,
					"name": "MNGO-PERP",
					"baseDecimals": 6,
					"baseLotSize": 1000000,
					"quoteLotSize": 100,
					"oracle": "79wm3jjcPr6RaNQ4DGvP5KxG1mNd3gEBsg6FsNVFezK4",
					"active": false,
					"settleTokenIndex": 0
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "ESdnpnNLgTkBCZRuTJkZLi5wKEZ2z47SG3PJrhundSQ2",
					"marketIndex": 2,
					"name": "SOL-PERP",
					"baseDecimals": 9,
					"baseLotSize": 10000000,
					"quoteLotSize": 100,
					"oracle": "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG",
					"active": true,
					"settleTokenIndex": 0
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Fgh9JSZ2qfSjCw9RPJ85W2xbihsp2muLvfRztzoVR7f1",
					"marketIndex": 3,
					"name": "ETH-PERP",
					"baseDecimals": 6,
					"baseLotSize": 100,
					"quoteLotSize": 1,
					"oracle": "JBu1AL4obBcCMqKBBxhpWCNUt136ijcuMZLFvTP7iWdB",
					"active": true,
					"settleTokenIndex": 0
				}
			],
			"tokens": [
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
					"tokenIndex": 894,
					"symbol": "JUP",
					"decimals": 6,
					"oracle": "ARXKK2gw7zpjonHbmcqYj7pQzwAxAa49Yy7kh5P44Aek",
					"mintInfo": "3PLpM8qWM8jsFBt2zREydfjC58rhPqiJV6LbQRywjHwA",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "BqHd7o8aHSmx6LtsV6onYQL5TbDrnXHabXkVXT4aRCfZ"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "45EgCwcPXYagBC7KqBin4nCFgEZWN7f3Y6nACwxqMCWX",
					"tokenIndex": 889,
					"symbol": "Moutai",
					"decimals": 6,
					"oracle": "AV67ufGVkHrPKXdeupXE2MXdw3puq7xnkPNrTxGP3suU",
					"mintInfo": "66B1a7RF9J9sAAnsBVMjSYTELKEx934eNk4CKdwLwEMZ",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "7W12TvXN4yoUwbYTRwMj754EvkFsZtX25MzvxSjeHxSL"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "BLZEEuZUBVqFhj8adcCFPJvPVCiCyVmh3hkJMrU8KuJA",
					"tokenIndex": 891,
					"symbol": "BLZE",
					"decimals": 9,
					"oracle": "AwpALBTXcaz2t6BayXvQQu7eZ6h7u2UNRCQNmD9ShY7Z",
					"mintInfo": "9bfz1uksfSFj1GbNQwEASxGjkTa5hPvFJjEPUQcrjPRa",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5NtZkGqDb9tw8iFeGeK7K5GkEiYkNRufxoQpksreJwyE"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "GFX1ZjR2P15tmrSwow6FjyDYcEkoFb4p4gJCpLBjaxHD",
					"tokenIndex": 893,
					"symbol": "GOFX",
					"decimals": 9,
					"oracle": "7UYk5yhrQtFbZV2bLX1gtqN7QdU9xpBMyAk7tFgoTatk",
					"mintInfo": "EAZ3ZiCVuc5Wj8b6XnGpEAoJqKLkq2ZHt4KS9TALsVzf",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "GhGPrMzAyKHgNNstHLWRVmsaxBQg44AGbzmS2ujuia5K"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "StepAscQoEioFxxWGnh2sLBDFp9d8rvKz2Yp39iDpyT",
					"tokenIndex": 916,
					"symbol": "STEP",
					"decimals": 9,
					"oracle": "9BoFW2JxdCDodsa2zfxAZpyT9yiTgSYEcHdNSuA7s5Sf",
					"mintInfo": "CKS1hKsWzFwcAPHbatvFkmPeCS5XW3BZTU6JmkftjjF5",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "7zGff2dhs5rXojd1sno4EM4cLdUN4Vp3vunSMgozAeFu"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",
					"tokenIndex": 472,
					"symbol": "RAY",
					"decimals": 6,
					"oracle": "AnLf8tVYCM816gmBjiy8n53eXKKEDydT5piYjjQDPgTB",
					"mintInfo": "8PUUwPHJKU6Dqb8LQmxMno13HoYBMzMkYqU7jmB1stW3",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "Hfgfto2NeAguUWyUkwPKDBg6E8K482VhVMQCkW3kV24k"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "85VBFQZC9TZkfaptBWjvUw7YbZjy52A6mjtPGjstQAmQ",
					"tokenIndex": 1010,
					"symbol": "W",
					"decimals": 6,
					"oracle": "AEkyHrj3X4xyJxvzMT96HeWi6qU2em1P7C8DgbdQ4pdG",
					"mintInfo": "8ExsYMgjyygLN5Z9a5e3Twg1ofLbDhTPLEtCnLDgEiN7",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "AEtZzoYGwCWGnXRAE8xXaEhisQyJBkRcXHpuzNVxXV57"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "DUALa4FC2yREwZ59PHeu1un4wis36vHRv5hWVBmzykCJ",
					"tokenIndex": 455,
					"symbol": "DUAL",
					"decimals": 6,
					"oracle": "7fMKXU6AnatycNu1CAMndLkKmDPtjZaPNZSJSfXR92Ez",
					"mintInfo": "Bp3Y2kseb3YKDd1WXEnevNKPzEwkBdgKLPL4FoGfkz1D",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "4hKKNTYxWEYWSqS87ZtKgFATSnretNhHRPzpANN11pCE"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj",
					"tokenIndex": 480,
					"symbol": "stSOL",
					"decimals": 9,
					"oracle": "Bt1hEbY62aMriY1SyQqbeZbm8VmSbQVGBFzSzMuVNWzN",
					"mintInfo": "A1EEkHxGFgpUjqLQocaPYPWiKs7bSkrwdjWgi12h6nYB",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5iTEQLB3qKQyUTsoyd1ULZCXGY7ATtv9CEk1LZEjm6J1"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "ZEUS1aR7aX8DFFJf5QjWj2ftDDdNTroMNGo8YoQm3Gq",
					"tokenIndex": 1024,
					"symbol": "ZEUS",
					"decimals": 6,
					"oracle": "4uFLuDiqL8M4d4ojLiE96ptGLFtj6CSr9PX2tHmv1K85",
					"mintInfo": "FpgQcaHBC4o9xd4a6gWHWTR2Dm2hfy4yrhYtd1WQz9YC",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5vEDYtU3vMh1nE3uxAC9QqnXFEkoMTGzDJYmmgKXcN6t"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "TNSRxcUxoT9xBG3de7PiJyTDYu7kskLqcpddxnEJAS6",
					"tokenIndex": 1025,
					"symbol": "TNSR",
					"decimals": 9,
					"oracle": "9yq5YVt8pwpcF1mQfVAS8j2gwsMv8sa6QTFLJ7uGbdUC",
					"mintInfo": "HXuMyw6ZGQ21YKtjzvgVVTERxV8bCjgvJchrfngi2qGe",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "GXtjxsVxx8SuKRBggnttAHa1YZcFaEiGShRuQQnPcZTD"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "METADDFL6wWMWEoKTFJwcThTbUmtarRJZjRpzUvkxhr",
					"tokenIndex": 1041,
					"symbol": "META",
					"decimals": 9,
					"oracle": "6xgRE2DWvyfxKTF1vzHApDXf6zExDEorkwozHNcKBwRX",
					"mintInfo": "GTd3e6sZWU6SwzaczVCB2f1jzQ7vvQKTdv3Wi6Jn2CkR",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "42w6EWt36LSnacXGyi5QuJLZA7JMh52kbACouCK3H5Ve"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7Q2afV64in6N6SeZsAAB81TJzwDoD6zpqmHkzi9Dcavn",
					"tokenIndex": 1063,
					"symbol": "JSOL",
					"decimals": 9,
					"oracle": "9uhLdpvKZyU5grG3fLAYzQAGpT9RHU2aYz2gyE2kYn4d",
					"mintInfo": "8AoQBB6uR625rERw9ufV1MAYyGN32vpaX1KGE3ymmuvk",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "AdhCNK7eytN8Ag2MBDT8NjUg1kopFqQ7UTgF9sWkEeEK"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7GCihgDB8fe6KNjn2MYtkzZcRjQy3t9GHdC8uHYmW2hr",
					"tokenIndex": 1069,
					"symbol": "POPCAT",
					"decimals": 9,
					"oracle": "2stQe1XLGkuTZ22gQrgZKsb93iG9mWXSLfANMPRjs5Ky",
					"mintInfo": "Ch1ySQxbGhDaxd44g7iLMBnXY23MtEDKSQdZtqgGEphw",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "JC3MttCscB1k3KZAxV2R3QBBsVPExPZjyhpCAjwxG4ZE"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "So11111111111111111111111111111111111111112",
					"tokenIndex": 4,
					"symbol": "SOL",
					"decimals": 9,
					"oracle": "H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG",
					"mintInfo": "EzQYaAhP3mdL4C8VQAAYxec9idCe31yCyAmzxWHLR4QJ",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "FqEhSJSP3ao8RwRSekaAQ9sNQBSANhfb6EPtxQBByyh5"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "EjmyN6qEC1Tf1JxiG1ae7UTJhUxSwk1TCWNWqxWV4J6o",
					"tokenIndex": 2,
					"symbol": "DAI",
					"decimals": 8,
					"oracle": "CtJ8EkqLmeYyGB8s4jevpeNsvmD4dxVR2krfsDLcvV8Y",
					"mintInfo": "A2EmRoXjscbwzbWS84rTnsvWwF2S9T7cdUNwshLWJcbf",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2AyZpjYWZf42ui1wzWJ642KWMCCY5YEU9GnKRWzQHLYW"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "BqVHWpwUDgMik5gbTciFfozadpE2oZth5bxCDrgbDt52",
					"tokenIndex": 634,
					"symbol": "OPOS",
					"decimals": 9,
					"oracle": "3uZCMHY3vnNJspSVk6TvE9qmb4iYVbrEWFQ71uCE5hFR",
					"mintInfo": "AGZEaSfS1aZfZvS5RAXqHbHfLhsifFYWki1apCv1n4e5",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "J9bsocpD25gr6AHjZ1nx4wHtmTC6SKN9op1iedSEmJ4K"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "GDfnEsia2WLAW5t8yx2X5j2mkfA74i5kwGdDuZHt7XmG",
					"tokenIndex": 645,
					"symbol": "CROWN",
					"decimals": 9,
					"oracle": "CttaiHm58dQgfFvKLgMkMxJuQsfwQsuEdRhq5a5bBLab",
					"mintInfo": "AUGDi8nsPBHU2MCLvAq1QDyXhGMyLhH9smmXVmF8oWuy",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "EkTJ96udE7jpwHi3Nog6Doj1QDFTpHbbU8ccPYF3dd88"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE",
					"tokenIndex": 520,
					"symbol": "ORCA",
					"decimals": 6,
					"oracle": "4ivThkX8uRxBpHsdWSqyXYihzKF3zpRGAUCqyuagnLoV",
					"mintInfo": "Abw5YTsXvJhvVEprNHeDJFgHv2HSJnqeyTTyu7NcFnYQ",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "F9Gym4CJUyYNcWJcRfBWBZaauzf2fx6U4EAGEej5HwGD"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
					"tokenIndex": 501,
					"symbol": "JitoSOL",
					"decimals": 9,
					"oracle": "8n3QYCX6HTBo5Po4dUagoRCzq2guXK6Y9fNtAyXSDrYA",
					"mintInfo": "FWrfYUYY7JRjdZx43GthgFQzHzFyHh5P2WbtgyqrR4cq",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "6LKYzJfyR3RxewQZRZRY1Sv8riqx9Q4AqUL7Qj8t4oZ3"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "AZsHEMXd36Bj1EMNXhowJajpUXzrKcK57wW4ZGXVa7yR",
					"tokenIndex": 669,
					"symbol": "GUAC",
					"decimals": 5,
					"oracle": "2qHkYmAn7HNtAGw45hQQkRthDDNiyVyVfDJDaw6iSoRm",
					"mintInfo": "FXMp99D2P7gjaBoHCRS35U79kmmLAfDwYuxAdRHyXQHV",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "ANCYKDGtxftEdazUNSt6o6keCPxkBWeXzgftiMjATWb4"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
					"tokenIndex": 1,
					"symbol": "USDT",
					"decimals": 6,
					"oracle": "3vxLXJqLqF3JG5TCbYycbKWRBbCJQLxQmBGCkyqEEefL",
					"mintInfo": "HPmQ7dvQpZV7yoRo8NqMS9jVZBJXocE472b1bbGoLG1Q",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "3k87hyqCaFR2G4SVwsLNMyPmR1mFN6uo7dUytzKQYu9d"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So",
					"tokenIndex": 5,
					"symbol": "MSOL",
					"decimals": 9,
					"oracle": "E4v1BBgoso9s64TQvmyownAVJbhbEPGyzA3qn4n46qj9",
					"mintInfo": "HopzURnbkLPpQch37o5sN31knjpYGrENuwy9UASXkhfC",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "AL2BeApHWeHdzERdiCy13ZrmPXaiBWG8s11deHrZPuSt"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",
					"tokenIndex": 7,
					"symbol": "BONK",
					"decimals": 5,
					"oracle": "8ihFLu5FimgTQ1Unh4dVyEHUGodJ5gJQCrQf4KUVB9bN",
					"mintInfo": "HweVqvYPKwcxb1vsFxz3qMWKu4wqYWBpCraTFvinJrZZ",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "CwyxwCugWhWDMZTo2xCjVr3CqjLorpBMpqHKZCLYpPod"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs",
					"tokenIndex": 3,
					"symbol": "ETH",
					"decimals": 8,
					"oracle": "JBu1AL4obBcCMqKBBxhpWCNUt136ijcuMZLFvTP7iWdB",
					"mintInfo": "J8Mq6JQUqaKfWjn9fjaYq8S5haCJXuUGtavjtQHzW4Vm",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5h2KcPQQijX1RR35yhBJPDzvrsLd4sUcTtXUq3PhVhY9"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "6DSqVXg9WLTWgz6LACqxN757QdHe1sCqkUfojWmxWtok",
					"tokenIndex": 792,
					"symbol": "CORN",
					"decimals": 7,
					"oracle": "2PRxDHabumHHv6fgcrcyvHsV8ENkWdEph27vhpbSMLn3",
					"mintInfo": "J9fFpFU3tsHrTTYv9NiUBBuQY622iYQPExbw68ryuiCN",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "8o518F5o8UiMjWfsTJZMnmCUQJdyzFanVfvWpAsEmyCj"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "BaoawH9p2J8yUK9r5YXQs3hQwmUJgscACjmTkh8rMwYL",
					"tokenIndex": 616,
					"symbol": "ALL",
					"decimals": 6,
					"oracle": "Ag7RdWj5t3U9avU4XKAY7rBbGDCNz456ckNmcpW1aHoE",
					"mintInfo": "JbxhwoCq8PyAqF4hrRNzsQifAhkovMxP4arWd41vkm9",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "DgHQBrroxQ4ahTuUcuZRz9LkvdDGiz35fMyFCEPX2Hr7"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "RLBxxFkseAZ4RgJH3Sqn8jXxhmGoz9jWxDNJMh8pL7a",
					"tokenIndex": 624,
					"symbol": "RLB",
					"decimals": 2,
					"oracle": "8v7etZJcPcx6VZqRFzZ7cuJmJYGso4Fd1ewgAtrrDKU7",
					"mintInfo": "2aFwFctfLdCkZwdDbE1YUpqZ6uNdUrfHToFBPfGscynD",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "6MS8gsFqdAoL1RTF8cPPdyCxUAfrofdHWCBGigwCUdpZ"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "3jsFX1tx2Z8ewmamiwSU851GzyzM2DJMq7KWW5DM8Py3",
					"tokenIndex": 601,
					"symbol": "CHAI",
					"decimals": 8,
					"oracle": "CtJ8EkqLmeYyGB8s4jevpeNsvmD4dxVR2krfsDLcvV8Y",
					"mintInfo": "987e6m4osFq72s9S5AApoH4bVA5JLQpERxqrrpR3Phbm",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "CMAwSRYuKgYdN7Q7uQVEWNfmViJACdRD3PPM5TLh1jJ2"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
					"tokenIndex": 0,
					"symbol": "USDC",
					"decimals": 6,
					"oracle": "Gnt27xtC473ZT2Mw5u8wZ68Z3gULkSTb5DuxJy7eJotD",
					"mintInfo": "9jPkVdufMKbg62ndJHv5CqCAkzssmstyDi9kxsvrjbSc",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "J6MsZiJUU6bjKSCkbfQsiHkd8gvJoddG2hsdSFsZQEZV"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
					"tokenIndex": 772,
					"symbol": "SAMO",
					"decimals": 9,
					"oracle": "5wRjzrwWZG3af3FE26ZrRj3s8A3BVNyeJ9Pt9Uf2ogdf",
					"mintInfo": "Ai6PUCxXgZdmuAMRHdXvqRKeqh6UyZEGWXCCtMxVbiRy",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2GxL3HkqjKijnX4KVX56JtA5ozBQPB7i3qyKd17nidGp"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "USDH1SM1ojwWUga67PGrgFWUHibbjqMvuMaDkRJTgkX",
					"tokenIndex": 588,
					"symbol": "USDH",
					"decimals": 6,
					"oracle": "BeAZ81UvesnJR7VVGNzRQGKFHrnxm77x5ozesC1pTjrY",
					"mintInfo": "B3CUeQ4aswx8rMGEBfqRHa2ByTcoRaCj47mUutnvWGPA",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "BBo3tPStXaeE8iUriGFw22u4z5m4b5RLUXdzufCcTwJw"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3",
					"tokenIndex": 719,
					"symbol": "PYTH",
					"decimals": 6,
					"oracle": "7vicWCSBLrdAENrjDJYefa5MupGyjs6XmYF8nxug2NcA",
					"mintInfo": "BGsUALwSZVkdbSVqTDM4jR5vND9YQcwNmXSU4uDrhKYB",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "H4x37jm2aU6YfekvJXmnBPKyLRNHcaoZq9nCBhJvDMds"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "kinXdEcpDQeHPEuQnqmUgtYykqKGVFq6CeVX5iAHJq6",
					"tokenIndex": 550,
					"symbol": "KIN",
					"decimals": 5,
					"oracle": "C7Q9t2YdEUgcd4ETZ8Egjc5gDjFmLw6Ak4WpYBtxLzxa",
					"mintInfo": "BM7rZgK4s7fKzRo9Pt6gEfgb9gta9YvS766R1BMYQDso",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "moJ2CjisbT4h77qg2bsYCfAGRaFcPmDRy7kzpQ9iVEs"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "nosXBVoaCTtYdLvKY6Csb4AC8JCdQKKAaWYtx2ZMoo7",
					"tokenIndex": 791,
					"symbol": "NOS",
					"decimals": 6,
					"oracle": "2FGoL9PNhNGpduRKLsTa4teRaX3vfarXAc1an2KyXxQm",
					"mintInfo": "BRuM7Dkt6GzNWRRNhKmrpKBPzuZLxLmYM7M8MNbgRJKM",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "66a4acM9iPxVc3GWj5tGkX2LL6JHUBKP1bNVdDnqSZjW"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "HZRCwxP2Vq9PCpPXooayhJ2bxTpo5xfpQrwB1svh332p",
					"tokenIndex": 489,
					"symbol": "LDO",
					"decimals": 8,
					"oracle": "ELrhqYY3WjLRnLwWt3u7sMykNc87EScEAsyCyrDDSAXv",
					"mintInfo": "Bm8CzFYxq9E4YEfaffdFrpRzgFTpiLER1JqBsevakezG",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "3rcQV5KqHiHU57e1KucLiQkUZnLcKaf6jq8SUufisENw"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "SLCLww7nc1PD2gQPQdGayHviVVcpMthnqUz2iWKhNQV",
					"tokenIndex": 741,
					"symbol": "SLCL",
					"decimals": 9,
					"oracle": "hnkVVuJTRZvX2SawUsecZz2eHJP2oGMdnhdDJa33KSY",
					"mintInfo": "Cw6QwcHG3kv4v22zXgt5dtzNwqugUHudvC8SVG2iPy6k",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "8zYg3soP6TyuHJLjByLZaj16HuKLd2wqKtAi3g1NgkfU"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof",
					"tokenIndex": 689,
					"symbol": "RENDER",
					"decimals": 8,
					"oracle": "CYGfrBJB9HgLf9iZyN4aH5HvUAi2htQ4MjPxeXMf4Egn",
					"mintInfo": "HsxS9Ba8fNq6j382ZaNtWCW3ZAmqJDfDcVva6ruXgGf6",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "4zNcntDJ114EHfocQCcqxJgRMxksTTu9NYHxfSXG42Qt"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4",
					"tokenIndex": 743,
					"symbol": "JLP",
					"decimals": 6,
					"oracle": "HW9PZxNqg7jnXftGPv9Mhhkw34Ek99B8AEt3YKred7KY",
					"mintInfo": "J7uE1tHcU8bWGt6BuAhe7NW7Mjo76wSefsnNTEDzvkCu",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "236A5DwzPZd3u2sRwRBYtabkLfPMnUquSzje271oPBF6"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "MangoCzJ36AjZyKwVj3VnYU4GTonjfVEnJmvvWaxLac",
					"tokenIndex": 6,
					"symbol": "MNGO",
					"decimals": 6,
					"oracle": "5xUoyPG9PeowJvfai5jD985LiRvo58isaHrmmcBohi3Y",
					"mintInfo": "2rbDQ2E1kDK4SHPbr1VUzB6sKdxkcJxDq8jYf44R4oVi",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5covW85GcCq3kHCJtcKCKYyQoxZLnHiz3zw5TEZEYgKj"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL",
					"tokenIndex": 720,
					"symbol": "JTO",
					"decimals": 9,
					"oracle": "7tQgYiykczx4WVXxjEjJSyPjyingYdaBVarLQznqPj4u",
					"mintInfo": "3opPQ2dBbNh7WZQChg8d1YPWGGZKfpNcVz3TJ5xdJCRd",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2axVS5vUVo2Hiy1MgcxGKJkHRxyuKofHyXBqvtGwRpiX"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",
					"tokenIndex": 805,
					"symbol": "$WIF",
					"decimals": 6,
					"oracle": "B7Gzb3BubnEHVtMNYaE1EagkTk9r6MLBnDLkGpWgdW9E",
					"mintInfo": "4R5iDjQKtAchyyY5PWvpcLZD4YQSrrkHUTdBHi3TFtP8",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "3QNuwYVDURQbjwKJoidMDVzCVR9NA3b3RqVJRVy3RWLe"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "3NZ9JMVBmGAqocybic2c7LQCJScmgsAZ6vQqTDzcqmJh",
					"tokenIndex": 8,
					"symbol": "wBTC (Portal)",
					"decimals": 8,
					"oracle": "GVXRSBjFk6e6J3NbVPXohDJetcTjaeeuykUpbQF8UoMU",
					"mintInfo": "59rgC1pa45EziDPyFgJgE7gbv7Dd7VaGmd2D93i1dtFk",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "8gabXzwdPn5TvtuQvysh3CxVbjfNY3TZd5XEG5qnueUm"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "hntyVP6YFm1Hg25TN9WGLqM12b8TQmcknKrdu1oxWux",
					"tokenIndex": 519,
					"symbol": "HNT",
					"decimals": 8,
					"oracle": "7moA1i5vQUpfDwSpK6Pw9s56ahB7WFGidtbL2ujWrVvm",
					"mintInfo": "6Ywaywi7wMbHgyLrRDybL3ccewWm8iRTWGUwL8tCMQZH",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "Fm9Vj9WBFePzEKCvqHpetU7HsGB3yzotGZ7KzrWuZj4"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "6DNSN2BJsaPFdFFc1zP37kkeNe4Usc1Sqkzr9C9vPWcU",
					"tokenIndex": 649,
					"symbol": "TBTC",
					"decimals": 8,
					"oracle": "CPMXpXwzuvLdHoaNFvXak13Hs6hbArD7PWD4wRLLfkK8",
					"mintInfo": "6ZdChWpjbqWK8AnvmDqXxJs2mTK7XG9MFNtEVcGhaq18",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "9uqM74oVmvusDS57LwrowGYy2nSeMbDHnutXWy5TF8J7"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "bSo13r4TkiE4KumL71LsHTPpL2euBYLFx6h9HP3piy1",
					"tokenIndex": 521,
					"symbol": "bSOL",
					"decimals": 9,
					"oracle": "EPw1Vb9YFu6TcasVKaj5mEUtvtz3G18iBdByqzigbzUG",
					"mintInfo": "6dfUTgFnWUtXiEHvkP4SP5TR2mEifh6NvUK4S43uoFEe",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "3YdNXbGDnb7MdEmFCpKmhRsYwBG8EtBxrfvGe2bmsZq1"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "MNDEFzGvMt87ueuHvVU9VcTqsAP5b3fTGPsHuuPA5ey",
					"tokenIndex": 847,
					"symbol": "MNDE",
					"decimals": 9,
					"oracle": "4dusJxxxiYrMTLGYS6cCAyu3gPn2xXLBjS7orMToZHi1",
					"mintInfo": "6r16fDZkNNjEd4w4GTMGLxf6S5y6CwvQh523F8CqJjUb",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2s8osfWrSADJPkGG6zc5t8ocXg213k9Ki7sZsank4e6H"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr",
					"tokenIndex": 777,
					"symbol": "EURC",
					"decimals": 6,
					"oracle": "91Sfpm86H7ZgngdGfAiVJTNbg42CXBPiurruf29kinMh",
					"mintInfo": "7A76hh7hLrvo2dmxg2Pc154LTk8yTq7wZBLMp6m3AsTC",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "9AZ2quj7pRTdUF4oA7GsT6xtiVY4YBT8RzW4LuSo4Aot"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "NeonTjSjsuo3rexg9o6vHuMXw62f9V7zvmu8M8Zut44",
					"tokenIndex": 716,
					"symbol": "NEON",
					"decimals": 9,
					"oracle": "FYghp2wYzq36yqXYd8D3Lu6jpMWETHTtxYDZPXdpppyc",
					"mintInfo": "7B2hAT84sjd9KW16X92hNjCsCcyqMe1yWEUjx2WrDWSj",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "47oGrbVDbTQXtv9NivR9eGobhAB53msgMhxq4TKqTfm5"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "WENWENvqqNya429ubCdR81ZmD69brwQaaBYY6p3LCpk",
					"tokenIndex": 848,
					"symbol": "WEN",
					"decimals": 5,
					"oracle": "Bfz5q3cDywSSjnWb9oXeQZqYzHwqFGp75mm34eYCPNEA",
					"mintInfo": "CKp8aGtzATLZH8upKwhf24AL2sHo2jdoGCxgXYKTHQG5",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2A49zq5LrkNowgmPwe7QNDUf1Nc4ZyJ4Avi1aEw3YJEc"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "6CNHDCzD5RkvBWxxyokQQNQPjFWgoHF94D7BmC73X6ZK",
					"tokenIndex": 881,
					"symbol": "GECKO",
					"decimals": 6,
					"oracle": "H5hokc8gcKezGcwbqFbss99QrpA3WxsRfqGYCm6F1EBy",
					"mintInfo": "DUuexjMRtQpv8Rbf1Wx3fFfTkovKy5S2YsymXM7M7gb6",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "75S1Jxo2BuGb4YnqVZzhWrgUroc4YJurUcKYUx6QjFk1"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7ZCm8WBN9aLa3o47SoYctU6iLdj7wkGG5SV2hE5CgtD5",
					"tokenIndex": 887,
					"symbol": "ELON",
					"decimals": 4,
					"oracle": "Ean9gZThDJn777burdR9vkZuCHTB5Kv7jrfcfNyo5K6J",
					"mintInfo": "99o7KDQAUxRBHS7emJPdCwNBDuLGCk8v7z3V9zR9iLeA",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5fzCKHsvDe3XbUKMtnDyqmAkdUVxYRx3ZN7WFV1Xiuu3"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "KMNo3nJsBXfcpJTVhZcXLW7RmTwTt4GVFE7suUBo9sS",
					"tokenIndex": 1075,
					"symbol": "KMNO",
					"decimals": 6,
					"oracle": "H8oLEoDyvABEDmGmXQuuzvSPWAkr2f2GKytbXiGX9YUm",
					"mintInfo": "HWy461RsEPVj6aSzavttCRNKbV75RJVXNgVabCZkYP4N",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2NkH1aLR8hqC9wPqTswn2Ls38awUSsQjfthPzq1x5ert"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "7BgBvyjrZX1YKz4oh9mjb8ZScatkkwb8DzFx7LoiVkM3",
					"tokenIndex": 1100,
					"symbol": "SLERF",
					"decimals": 9,
					"oracle": "8LxP1juSh9RPMECQiTocqk8bZcrhhtqgUEk76y4AmE2K",
					"mintInfo": "BeBWHqKzKZob5Lamw4e6uYEDDXZc9F381AmmQCujVU3z",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "6V4SoXme88WVgLHK7YB14ri8dCZcq7Vm3McVunHE75d5"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "ukHH6c7mMyiWCf1b9pnWe25TSpkDDt3H5pQZgZ74J82",
					"tokenIndex": 1101,
					"symbol": "BOME",
					"decimals": 6,
					"oracle": "JDj6n1iBeJUB54rNsmKw9ty2psAnkcXySLRshBWrYfGD",
					"mintInfo": "G8bPipGncWEoeyKEN4vKc5nnG8Cw8hLmG25iyUMtH5w6",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "4j19G8YzqTuj1i8kV6ZrANh7jTMHNmrXAGZ4MQVmgG54"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "PUPS8ZgJ5po4UmNDfqtDMCPP6M1KP3EEzG9Zufcwzrg",
					"tokenIndex": 1102,
					"symbol": "PUPS",
					"decimals": 9,
					"oracle": "ApF6hz2W7FSKMgmmpWxLm6ijA2J5vU2XDBaBLvjbyMbm",
					"mintInfo": "3KwuM2vQth4ic3KqahJGsY9y4kM6ThQykgSPUMn8dTQg",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "267y6qNubSwfuZivsGqaN4ed4ukzzVbw6LATzNwTtWKq"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "MEW1gQWJ3nEXg2qgERiKu7FAFj79PHvQVREQUzScPP5",
					"tokenIndex": 1103,
					"symbol": "MEW",
					"decimals": 5,
					"oracle": "BogEXWj8YcrXV31D7FzzUmTCS1hFHfRGZN7rnVN1Vcqe",
					"mintInfo": "96TZWR744jBHqYipxydDjHXaEfLYjdhZN8us8bGkfVeG",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "7YMNMBh2XtiWXCgAo1tDYPJhA1rzr1xZXZmZ8n1mHCiB"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "5oVNBeEEQvYi1cX3ir8Dx5n1P7pdxydbGF2X4TxVusJm",
					"tokenIndex": 1105,
					"symbol": "INF",
					"decimals": 9,
					"oracle": "HxoRWNTqw6BUvQVKbxfu3JxgBjHb7dJPr43bUnC2mopb",
					"mintInfo": "5PXeg8Hz4c73Y8TybAUsMZZRRqQtxZrKgwtBMSBm2qq2",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "9smCJL4hJKodptzwT9ZgK6YEF9gBpJxrh2fDCuZxoobY"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "8wXtPeU6557ETkp9WHFY1n1EcU6NxDvbAggHGsMYiHsB",
					"tokenIndex": 1110,
					"symbol": "GME",
					"decimals": 9,
					"oracle": "B9BzQ6hBBFn3C6fsGsVwcFd1v5cdbAwi8bUNmL58Bx8z",
					"mintInfo": "5Qq3VohUArrDQjv5vxpTmoFEVYUj4ErkyLm7neTFK7kg",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "FPqo1aQBWcPE5rQKqzHMU2f6oozEa7wKJ3VGFD9xwRuq"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "DriFtupJYLTosbwoN8koMbEYSx54aFAVLddWsbksjwg7",
					"tokenIndex": 1113,
					"symbol": "DRIFT",
					"decimals": 6,
					"oracle": "HtXYEquNkHJzGZmtCi8V4NQGTFD4hDebA57wziZWh8CV",
					"mintInfo": "BTjJxjZpbwgTa6QurKeTVpf2vhyRVEnUgh9KnxyoqKZZ",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "8kKH96jiTGxjmdNUz3EXmrivoZbNGznnpuewVPK1YYUg"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "HUBsveNpjo5pWqNkH57QzxjQASdTVXcSK7bVKTSZtcSX",
					"tokenIndex": 1153,
					"symbol": "hubSOL",
					"decimals": 9,
					"oracle": "AaSajgj4EuQ1SRWwWqiZCJedhfpWPmFmFntTUHRJmKpM",
					"mintInfo": "HjH5xAvSaguNr4JsyJxrb7Ujv55GX4WXBn4Z6zcVpM4n",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "AzYzvzecpTpqkqw5V89Gi2o9CfXYrEGD5oWq9GEBztZG"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "3S8qX1MsMqRbiwKg2cQyx7nis1oHMgaCuc9c4VfvVdPN",
					"tokenIndex": 1156,
					"symbol": "MOTHER",
					"decimals": 6,
					"oracle": "AKVzthVS5FbJrP4fFm68NgCdztcJJdwn4R5tYPxn65Cd",
					"mintInfo": "Cu1hzJHheEct5AN2K8rybCxkY19ue774sh193wQE3DCw",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "2phrN66dJfExn1JAZhdtY3HBF9Ms3EozUh6QmXKgNhTt"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "DUAL6T9pATmQUFPYmrWq2BkkGdRxLtERySGScYmbHMER",
					"tokenIndex": 1158,
					"symbol": "dualSOL",
					"decimals": 9,
					"oracle": "2CVNbYuRVeMfssifeg5NezcT14TBPjC2YLxHvJxFYxK2",
					"mintInfo": "23i7JzBAcsCoAHjMMVVpXn8CzWvV8WhFmeANpskjJ6KM",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "EijGccy8d4VMNQCDGGYqRcqxV6XLdKsmcXG1kuEGWKTz"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "D1gittVxgtszzY4fMwiTfM4Hp7uL5Tdi1S9LYaepAUUm",
					"tokenIndex": 1161,
					"symbol": "digitSOL",
					"decimals": 9,
					"oracle": "8FuGVt99koXrmmPe9NBy3FxenuQPAa3wqH3XzdRzz7UH",
					"mintInfo": "bNJWbUBBecpy8CxXLJe77ZkYL2ALDzvMoMeubJmXTNH",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "5BtcnYuAztkwrPKjRuWpPVEMutTfmDYRrYdxr7FsCbgd"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "MangmsBgFqJhW4cLUR9LxfVgMboY1xAoP8UUBiWwwuY",
					"tokenIndex": 1162,
					"symbol": "mangoSOL",
					"decimals": 9,
					"oracle": "zdTvr8Dbcm66D5aCk7XrHQ5qz3F8tTn28TvMqGhQkNe",
					"mintInfo": "BdPPhWsWM6GkuTEGZv93pWp5gLeUMf7We4cghDxdMUHR",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "3aZgVCtD2LnvYvBhDov26KtZEFHerz6GcuSPf3V9632Z"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "Comp4ssDzXcLeu2MnLuGNNFC4cmLPMng8qWHPvzAMU1h",
					"tokenIndex": 1163,
					"symbol": "compassSOL",
					"decimals": 9,
					"oracle": "GnGbhoGJgpoYEMFJncw7at8bfxZtEo3ggcPnXinMsq8b",
					"mintInfo": "3jbkfTd5XPRHPpeLH7hEyUUCWTAFD9bGB2fQMMP194yN",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "AhC3zouv7QxyiFcQd2mubYMum1UqDpLfR1HpaN6JVvLX"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "3B5wuUrMEi5yATD7on46hKfej3pfmd7t1RKgrsN3pump",
					"tokenIndex": 1173,
					"symbol": "BILLY",
					"decimals": 6,
					"oracle": "DKt5kYg2wcY3SpbMZrYcJUg23mwEEQ2PsCioyPfcX633",
					"mintInfo": "FQqyKgJu42kA4qh9gknVm9pbVumzVwCiyae64EPSWnzR",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "Gu6dz7oLm9ns77koGgF413piPmUyUDmXmPJ1q8Ln7Cbu"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "A1KLoBrKBde8Ty9qtNQUtq3C2ortoC3u7twggz7sEto6",
					"tokenIndex": 1179,
					"symbol": "USDY",
					"decimals": 6,
					"oracle": "5RKJ9unGQQhHezsNg7wshfJD4c5jJ64iXYu1nk6PJ5fb",
					"mintInfo": "Ac85NFTP6L3XomGtmhhfJZ68d6nznQUbb2AKQtYpNmzV",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "zv6F3XvHWcLLDs2DwKUYBHvFettsXQxetv2pfePF9EB"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "Ds52CDgqdWbTWsua1hgT3AuSSy4FNx2Ezge1br3jQ14a",
					"tokenIndex": 1189,
					"symbol": "DEAN",
					"decimals": 6,
					"oracle": "GBLmxoJzQrwPLQttMTsQZRVMpBbxUtdMxYfLrXtkbun9",
					"mintInfo": "HjssXzmoT5KxSAaxW5HSfcqy9JhosnGoFegr9hjbTF6e",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "3tR6XQhvy9tU6DMFkdDWk6LGjksK4sswstZtvS8odJnS"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "9pYxF6BZPN98D3DQHt8nqGsudTmLHhy9PT5yDopjUXJd",
					"tokenIndex": 1199,
					"symbol": "LNGCAT",
					"decimals": 6,
					"oracle": "H5DimRdrm4xjMMEzg574QKkfaHZcraGLqC85JJ4PBm58",
					"mintInfo": "2gUiRNwjZinuTgZjcHGLnkZYAoXeznJr9PKrNGHb11rk",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "CsSzH1w8wwuYk4La9B4J6cFxty7zSCsC61z9UM5p1RyF"
						}
					],
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "StPsoHokZryePePFV8N7iXvfEmgUoJ87rivABX7gaW6",
					"tokenIndex": 1252,
					"symbol": "stepSOL",
					"decimals": 9,
					"oracle": "7jdeNZ4ppZ2P3FQsmoJHzcNSZJfBRPzbYkGKNFhb5L3n",
					"mintInfo": "Dmkq6a7aWXqJr2jxGipXQcN6aK2YcyhitqQuDbQVjuEH",
					"banks": [
						{
							"bankNum": 0,
							"publicKey": "4CBGok8YFhT8F2WSztyP3GfoaHacZMpiCXhLfSrYb7fH"
						}
					],
					"active": true
				}
			],
			"stubOracles": [
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
					"publicKey": "7e7xcRkdb4uJ4A6xShYwBJrWJk5V4AmrEFqAPhSMvzSx"
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"mint": "jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL",
					"publicKey": "7tQgYiykczx4WVXxjEjJSyPjyingYdaBVarLQznqPj4u"
				}
			],
			"serum3Markets": [
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "8JTrmcsZYABLL2HQcNnFo7q7osCVAsRW7m9ggE9Dj9Dw",
					"marketIndex": 0,
					"name": "SOL/USDC",
					"baseTokenIndex": 4,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "8BnEgHoWFysVcuFFX7QztDmzuH8r5ZFvyP3sYwn1XTh6",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Dp6vp6PfK29gvUMxbjMLhZHiTTFuiStGqvLrZNovvk17",
					"marketIndex": 5,
					"name": "wBTCpo/USDC",
					"baseTokenIndex": 8,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "3BAKsQd3RuhZKES2DGysMhjBdwjZYKYmxRqnSMtZ4KSN",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Cq3Hs8WYcVHfvvnz1nv8MnAfZJGBrgj1PAhLtxYZwEBJ",
					"marketIndex": 6,
					"name": "MNGO/USDC",
					"baseTokenIndex": 6,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "3NnxQvDcZXputNMxaxsGvqiKpqgPfSYXpNigZNFcknmD",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "3FcHyWXDcqzrPCPEQyohBfJxZPp5ZKgm8mG93xHcUuCb",
					"marketIndex": 7,
					"name": "BONK/SOL",
					"baseTokenIndex": 7,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "Hs97TCZeuYiJxooo3U73qEHXg3dKpRL4uYKYRryEK9CF",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "98WyndeGfWdCypzrAoHUZU1pjiznR2YiNVYDCxsTVF4u",
					"marketIndex": 455,
					"name": "DUAL/USDC",
					"baseTokenIndex": 455,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "H6rrYK3SUHF2eguZCyJxnSBMJqjXhUtuaki6PHiutvum",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "6M7Zrd9UtWc8bhfa6zVnsuN9QULskY6x5bPcZ543hVUQ",
					"marketIndex": 2,
					"name": "mSOL/USDC",
					"baseTokenIndex": 5,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "9Lyhks5bQQxb9EyyX55NtgKQzpM4WK7JCmeaWuQ5MoXD",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "7uMfph4Ho4EU3uupS59ajZGdYpVfWHK2pZ2dCFi1jk9M",
					"marketIndex": 472,
					"name": "RAY/USDC",
					"baseTokenIndex": 472,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "DZjbn4XC8qoHKikZqzmhemykVzmossoayV9ffbsUqxVj",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "GZF2rs2ArVDYiJeuiQq9db6EKrZC1Wk63BwmuisUiNmE",
					"marketIndex": 480,
					"name": "stSOL/USDC",
					"baseTokenIndex": 480,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "JCKa72xFYGWBEVJZ7AKZ2ofugWPBfrrouQviaGaohi3R",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "2VUr6NE7q12tH9eRgcZdyu4ZmkHYWNScoYKQ3ZxjBFwi",
					"marketIndex": 489,
					"name": "LDO/USDC",
					"baseTokenIndex": 489,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BqApFW7DwXThCDZAbK13nbHksEsv6YJMCdj58sJmRLdy",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "J62mm6kS5qh2TDzwyCCLsuxYne5TFt5WnTvWwwCg1wCY",
					"marketIndex": 499,
					"name": "stSOL/SOL",
					"baseTokenIndex": 480,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "GoXhYTpRF4vs4gx48S7XhbaukVbJXVycXimhGfzWNGLF",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9MkAUddqZsLVHWxxcFBGNCbrrKsnRQUsPxFrUr5pnyvD",
					"marketIndex": 501,
					"name": "JitoSOL/USDC",
					"baseTokenIndex": 501,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "DkbVbMhFxswS32xnn1K2UY4aoBugXooBTxdzkWWDWRkH",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "CVkEVS8hyv4kPvKAiRpeaJ3raefsVEfG7jYHQrqU2f9M",
					"marketIndex": 515,
					"name": "jitoSOL/SOL",
					"baseTokenIndex": 501,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "G8KnvNg5puzLmxQVeWT2cRHCm1XmurbWGG9B8Bze6mav",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "H43ud12qMunjGF23UGov8ps5cPdiZ3xSARNHp7Uj2J5e",
					"marketIndex": 502,
					"name": "USDT/USDC",
					"baseTokenIndex": 1,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "B2na8Awyd7cpC59iEU43FagJAPLigr3AP3s38KM982bu",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9RNhAJpWNaYoT9YdXVYDKzAzq6qAXLXboawdvoroAoXm",
					"marketIndex": 519,
					"name": "HNT/USDC",
					"baseTokenIndex": 519,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "CK1X54onkDCqVnqY7hnvhcT7EosnjiLTwPBXAMLxkA2A",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "8tnuep1LYfxd1htXj6nSB1jKeJyBywiL8BMFrcbxCjwJ",
					"marketIndex": 520,
					"name": "ORCA/USDC",
					"baseTokenIndex": 520,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BEhRuJZiKwTdVTsGYjbHRh9RmGbKBtT6xo7yPqxLiSSY",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "ivX8pNxiiTn9geDUYFR1SbJZ6Xzhyuz4kQNTGasHcbo",
					"marketIndex": 521,
					"name": "bSOL/USDC",
					"baseTokenIndex": 521,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "ARjaHVxGCQfTvvKjLd7U7srvk6orthZSE6uqWchCczZc",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "HJFXU2X3fgMtsSpZqDioaGNyCGk1oGPywfhRUcjri8ja",
					"marketIndex": 523,
					"name": "bSOL/SOL",
					"baseTokenIndex": 521,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "6QNusiQ1g7fKierMQhNeAJxfLXomfcAX3tGRMwxfESsw",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "CjjmHLg9tLFZ86jun4Gbi9c6r6ndZqEiVTevhAf3br7x",
					"marketIndex": 1,
					"name": "ETH/USDC",
					"baseTokenIndex": 3,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "FZxi3yWkE5mMjyaZj6utmYL54QQYfMCKMcLaQZq4UwnA",
					"active": false
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "BpdPdB2NPb1ZdU96cGtM5SV7Hhokc9aRaCRSHNXpJQaY",
					"marketIndex": 550,
					"name": "KIN/USDC",
					"baseTokenIndex": 550,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "4WeAXG1V8QTtt3T9ao6LkQa8m1AuwRcY8YLvVcabiuby",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "7vvxFMVBfMqvzxJRtbaXFjanNZrfsqj9shy2KcN1j2gd",
					"marketIndex": 588,
					"name": "USDH/USDC",
					"baseTokenIndex": 588,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "6wD9zcNZi2VpvUB8dnEsF242Gf1tn6qNhLF2UZ3w9MwD",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "8qzdWt4HZgb27GpWLk7FyYBryfCmLMPMQN9EvLJCheVb",
					"marketIndex": 601,
					"name": "CHAI/USDC",
					"baseTokenIndex": 601,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "7S2fEFvce5n9hGpjp9jd8JRfuBngcDJfykygeqqzEwmq",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "E9fUansnDDCBXoWaqWqJ7XvATyUDkSwgs3cqwvFTj4eQ",
					"marketIndex": 616,
					"name": "ALL/USDC",
					"baseTokenIndex": 616,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "EN41nj1uHaTHmJJLPTerwVu2P9r5G8pMiNvfNX5V2PtP",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "FnDVAsHvy3yM8PYayqVVnuGciLRp7zfBUjJEQBHHumsH",
					"marketIndex": 624,
					"name": "RLB/USDC",
					"baseTokenIndex": 624,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "72h8rWaWwfPUL36PAFqyQZU8RT1V3FKG7Nc45aK89xTs",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "8aDUjXriDLNbvmPzVaS8ZwRng5pRERAGWidg1jfc1un1",
					"marketIndex": 617,
					"name": "BONK/USDC",
					"baseTokenIndex": 7,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "7tV5jsyNUg9j1AARv56b7AirdpLBecibRXLEJtycEgpP",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Da4TtMXAKv9uf8sNvhco3agbV9Bvj9S6otgG3Xf9yCv3",
					"marketIndex": 3,
					"name": "ETHpo/USDC",
					"baseTokenIndex": 3,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BbJgE7HZMaDp5NTYvRh5jZSkQPVDTU8ubPFtpogUkEj4",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "4JptAVNgDYqn8XPcPQgVK8sy99bjyDuv5aTNnFuU8vC9",
					"marketIndex": 4,
					"name": "BONK/USDC",
					"baseTokenIndex": 7,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "8PhnCfgqpgFM7ZJvttGdBVMXHuU4Q23ACxCvWkbs1M71",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "5CBZZrfhtVZjx6GfJQUh1bDxnoyzhB9LZHac75Mefhk8",
					"marketIndex": 645,
					"name": "CROWN/USDC",
					"baseTokenIndex": 645,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "HDwpKCNpB9JvcGrZv6TWcXjFvzxxfzq7ci6kQ1Kv8FMY",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "GDxceWPPqakaNGaudmPtWG4oUw66s33PkP3o9L5mZHCs",
					"marketIndex": 649,
					"name": "TBTC/USDC",
					"baseTokenIndex": 649,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "6nh2KwhGF8Tott22smj2E3G1R15iXhBrL7Lx6vKgdPFK",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "4DEaYSKHocgHZHhkZHNLz7cCaQ9dhUHu5sF6WABBH7nE",
					"marketIndex": 650,
					"name": "GUAC/USDC",
					"baseTokenIndex": 669,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "63XwffQkMcNqEacDNhixmBxnydkRE3uigV7VoLNfqh9k",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "P7sL2tVx5Gn6DENFv4XjRJasw1q1c51Z7DSv9MsMtEW",
					"marketIndex": 791,
					"name": "NOS/USDC",
					"baseTokenIndex": 791,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "9LezACAkFsv78P7nBJEzi6QeF9h1QF8hGx2LRN7u9Vww",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "2nfykM9uJTYUQjAzPUd4t5n1xBpBNh6RQ87Bkv7zC1ee",
					"marketIndex": 473,
					"name": "mSOL/SOL",
					"baseTokenIndex": 5,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "AYhLYoDr6QCtVb5n1M5hsWLG74oB8VEz378brxGTnjjn",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "2c9MZ1a9ysMBNhAPrHT5ZPJGZ6BL5xfxcmW7Ht4HrVoG",
					"marketIndex": 689,
					"name": "RENDER/USDC",
					"baseTokenIndex": 689,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "6XsUQYAkKSy4mSQfMxYqpF4U7X3JsPDbG4vRQQEvCPb6",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "CeoQTaEaw9E3AdyuBzU7Ue9usgBWmGXkfjR2Phchifoh",
					"marketIndex": 716,
					"name": "NEON/USDC",
					"baseTokenIndex": 716,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "Fb5BfdB7zk2zfWfqgpRtRQbYSYERASsBjz213FaT461F",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "DJut4JkAUnEiSWXYLgVKBb5fu3cRcX3Nd3kmJMCiKBNM",
					"marketIndex": 719,
					"name": "PYTH/USDC-OLD",
					"baseTokenIndex": 719,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "4E17F3BxtNVqzVsirxguuqkpYLtFgCR6NfTpccPh82WE",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9uj1TtuhUrJ7KHX43YKRSX64QYEMY68r9z5YRhzxkCJq",
					"marketIndex": 720,
					"name": "PYTH/USDC",
					"baseTokenIndex": 719,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "EA1eJqandDNrw627mSA1Rrp2xMUvWoJBz2WwQxZYP9YX",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "7zEXKLujD7JwWmn7tjYoVwMAyN1jEW3jsxQpejVE5969",
					"marketIndex": 741,
					"name": "SLCL/USDC",
					"baseTokenIndex": 741,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "AuqKXU1Nb5XvRxr5A4vRBLnnSJrdujNJV7HWsfj4KBWS",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "DawXBNofwMTq4hucH4M21tJfRfRyAVga6ooXSP767hiQ",
					"marketIndex": 742,
					"name": "JTO/USDC",
					"baseTokenIndex": 720,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "H87FfmHABiZLRGrDsXRZtqq25YpARzaokCzL1vMYGiep",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Actj5j7TksXuzggPTp6X9m5A99HZPhtXYCrdBj1i2xag",
					"marketIndex": 772,
					"name": "SAMO/USDC",
					"baseTokenIndex": 772,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "E5AmUKMFgxjEihVwEQNrNfnri5EexYHSBC4HkicVtfxG",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "G9kcUYZZe27PsUBkhvyakv9tuykjgTeSQiE2fc6bGxJS",
					"marketIndex": 777,
					"name": "EURC/USDC",
					"baseTokenIndex": 777,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "H6Wvvx5dpt8yGdwsqAsz9WDkT43eQUHwAiafDvbcTQoQ",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "68EhGhmimfHoBgMsPsuqqoHXzsBZ8QTNABuVEhLMVPma",
					"marketIndex": 795,
					"name": "CORN/USDC",
					"baseTokenIndex": 792,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "2mBnnBywAuMwH5FhH27UUFyDGk7J77m5LcKK4VtmwJQi",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "FfMSQoPYWqdu8a1abSyHMvVTofXTuYxNKmm96PAENded",
					"marketIndex": 805,
					"name": "$WIF/USDC",
					"baseTokenIndex": 805,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "CeQ7wj43PJ28EXU1QVNMPxmwrg955KejYD68bMYWTvAp",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "6jV1F1x51CPjy5Do7tny4NNGDzDS2HS2ZQjMdpSmrZ6A",
					"marketIndex": 820,
					"name": "TBTC/WBTCpo",
					"baseTokenIndex": 649,
					"quoteTokenIndex": 8,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "3rQH87K3UfrDjbjSktHy7EwQHvX4BoRu3Py52D25gKSS",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "2gJTUEm5tSNpwe7mAxWhfGmYLUFDqBkpb3T4RbicTKb3",
					"marketIndex": 806,
					"name": "RENDER/USDC",
					"baseTokenIndex": 689,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "2m7ZLEKtxWF29727DSb5D91erpXPUY1bqhRWRC3wQX7u",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "4fKhB4PptNxma87CYvKM7Gn676AnqACeptfRCFVk3Fjk",
					"marketIndex": 807,
					"name": "WIF/USDC",
					"baseTokenIndex": 805,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "2BtDHBTCTUxvdur498ZEcMgimasaFrY5GzLv8wS8XgCb",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "SbsvgDAxeqVYTCV183GjK5JT7tGjjirbejMaTnDBhZ4",
					"marketIndex": 847,
					"name": "MNDE/USDC",
					"baseTokenIndex": 847,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "CC9VYJprbxacpiS94tPJ1GyBhfvrLQbUiUSVMWvFohNW",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "4BBsYPavRhS9azA1An7Yq4zFkTPoqWJzxjALtQCt2Q1Z",
					"marketIndex": 856,
					"name": "JLP/USDC",
					"baseTokenIndex": 743,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "ASUyMMNBpFzpW3zDSPYdDVggKajq1DMKFFPK1JS9hoSR",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Azn5enE4vpfnvnM4KrF1xBSDpfd7g3MYp1GEzvNeNDyz",
					"marketIndex": 881,
					"name": "GECKO/USDC",
					"baseTokenIndex": 881,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "8QCdRwLp5CX2XYVaKX3GFxsbc8n7M2xEtMXyAa8tL7r3",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "3HdsJfdcszfFQWtXieF2JRe24zD3YoYfsf2vxyWqXNhN",
					"marketIndex": 857,
					"name": "WEN/USDC",
					"baseTokenIndex": 848,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "2oxZZ3YXaVhbZmtzagGooewBAofyVbBTzayAD9UR1eBh",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "72tcvnnkRT26Pz46tpGZprFuY4xHq3QM9EbAvpFZXF7K",
					"marketIndex": 894,
					"name": "JUP/USDC",
					"baseTokenIndex": 894,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "FbwncFP5bZjdx8J6yfDDTrCmmMkwieuape1enCvwLG33",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "GwkV85bmvFTqZQRXLAFKQonqbCF76HMTbdHWCQq5veyp",
					"marketIndex": 889,
					"name": "Moutai/USDC",
					"baseTokenIndex": 889,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "74fKpZ1NFfusLacyVzQdMXXawe9Dr1Kz8Yw1cw12QQ3y",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "3W9uzvEFnwJ1jXDoJjXVuTPR1Ef2uAJ6Pt7RrFpXGQfr",
					"marketIndex": 891,
					"name": "BLZE/USDC",
					"baseTokenIndex": 891,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "GFJjJmm7jTDb7WEM4TkYdA9eAEeJGK1t73tcdDNeZLGT",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Qum72miocnFKwiaRJGjfrv6jdZz2NPYEvrrGwDMooBF",
					"marketIndex": 893,
					"name": "GOFX/USDC",
					"baseTokenIndex": 893,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "9FjM1wHvGg2ZZaB3XyRsYELoQE7iD6uwHXizQUDKRYff",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "ChwUw6SAUEUsfLw2dLwAfRyENNhFt9G1JxWNjnGTgFo5",
					"marketIndex": 916,
					"name": "ELON/USDC",
					"baseTokenIndex": 887,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "AmFXLH3jbcQNqgJjVuMZCeiaU2HmrW1UwMTWR5wU4ijd",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9hEotRzswhgP3zEuNbN8sHDWjM2ZDqTPNZmMRS7b7DsP",
					"marketIndex": 895,
					"name": "ELON/USDC",
					"baseTokenIndex": 887,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "CDm1Uaos4vWPXezgEobUarGJ6ddKCywvFp8XLcNSqzU9",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9JnR1CYaJ6CQgbD8pfgdmCgRv67qvPQ4C2VocPx1G17a",
					"marketIndex": 926,
					"name": "STEP/USDC",
					"baseTokenIndex": 916,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "CCepXEQxo8eTqCGtRHXrSnZdhCEQjQeEW3M85AH9skMJ",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "ApNhd1XeVzpJW7D8PBUL8TjAttA2XuxqgPzBzahBbBD6",
					"marketIndex": 1010,
					"name": "W/USDC",
					"baseTokenIndex": 1010,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BLsemfNpXeZdfiHirneuvMpHooa2czxHevzRGLHPtbDu",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9bba1EumK9RZkjoKRZNXyZEXt65P7eR5WYfnfWty27q8",
					"marketIndex": 1024,
					"name": "ZEUS/USDC",
					"baseTokenIndex": 1024,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "GD6FoKTqBgRKDv9T3DCTNh4HqzxRgGKobnaR84JRmoUy",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "4YM6PqaxXjD8PCgZXhkf7VKc1mWvkvQk9Vcriz5TjKvL",
					"marketIndex": 1025,
					"name": "TNSR/USDC",
					"baseTokenIndex": 1025,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "Ds7p1pXbJvvgBABrUq6BoSRehDAGeUqgjTu4SSozxiJ9",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "FUPadFYvj8YAb3zxSfCoNLPXf269TgH69pUvAUXRA2Ba",
					"marketIndex": 1041,
					"name": "META/USDC",
					"baseTokenIndex": 1041,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "3mvtpi9JnPKaiRAdtY7sAXKAssf58wXS7wdqhpiu8BTn",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "5PMBip2biqvbo5D5keV2DKBFeAqqZnkoudYo4zVpnzvd",
					"marketIndex": 1063,
					"name": "JSOL/SOL",
					"baseTokenIndex": 1063,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "J45chsrgXviN5q6HLzfZNHCam2eJQprsJfZqJWVipfhm",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "KEC5M1D4e3bdmnAeRvgT2DxzGkKSfPcneWMzCsfCAXp",
					"marketIndex": 1069,
					"name": "POPCAT/USDC",
					"baseTokenIndex": 1069,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "5wchoBr91tdDpK4qTMFhhaYh4YGQ3t22G7LYN9ugQ7tZ",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "Bbgwy6f3EBqXY7o13kPbQ8yTtnj31aiwRxWPsvQFNV8V",
					"marketIndex": 1075,
					"name": "KMNO/USDC",
					"baseTokenIndex": 1075,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "8W728M651GnYA4eQAJs23GUc7PqscXGzC46NziR8VNHg",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "EXrsttBmFQE3rCW1m9SQqrUFPr6Rq2qnfPP51S6cHAaR",
					"marketIndex": 1100,
					"name": "SLERF/USDC",
					"baseTokenIndex": 1100,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "5pU6CRNbpkTLiBA9bUGDMrhSNLwnnZcEr8xCpSTzw5wF",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9jbRqZP4p13RAAfShgQPxtnXdWGrZJjZ7yTr5pERKmFE",
					"marketIndex": 1101,
					"name": "BOME/USDC",
					"baseTokenIndex": 1101,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "7WwnhPzcFLAaJo6pmkAop2XEWhCCVwC2b4UPx6GkbbJi",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "FpVATW5PKHjpoL5jAumv9mGxSr2Y5bF1Jj1mY4ybwH7D",
					"marketIndex": 1102,
					"name": "PUPS/USDC",
					"baseTokenIndex": 1102,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "B3REJ7w3brMmRFKi4zkXT4gALgotLtHokY1j7zKr27Uc",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "3qrS7KX6tDNGZS9u4M467Mq2jvEYL83kcB42Gkxhmbv9",
					"marketIndex": 1103,
					"name": "MEW/USDC",
					"baseTokenIndex": 1103,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "ECWFihVjC6yguzg6B7JuzoPPrSxCrk2jPwRpaW9ALdSV",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "5TjG96d66VHW3A43Nz6TScAzNenHZHc5SCg7iLC8oTQ7",
					"marketIndex": 1105,
					"name": "INF/SOL",
					"baseTokenIndex": 1105,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "2eSZySzRb6w7RrwbgVgrcPYLvtp5v8jBjsp387FbFtNn",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "CV1vnq2DFBoPNyzYK5zzowttY6uCipx78SeKRoLNd8f4",
					"marketIndex": 1110,
					"name": "GME/USDC",
					"baseTokenIndex": 1110,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "AyDycXWY2ykfQTjichyaKTNcU79KtojduYn4FtfuY2Fm",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "7f8FCNsEsAWTnXL8scz4goVTSvrJJYDwG97MGkAVKSUj",
					"marketIndex": 1113,
					"name": "DRIFT/USDC",
					"baseTokenIndex": 1113,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BTEutgKbxb8tE3hi9ZRYEx678HXTktkRicRviY9xJ9nf",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "BPLmjDrcFSkCznLQbbZon5dMX2CGshToQX9pp8YFXwVa",
					"marketIndex": 1153,
					"name": "hubSOL/SOL",
					"baseTokenIndex": 1153,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "8Pcobd6mTd7Av4GkmPZLq95wJTXD7cmV6UnyJSe6yVFA",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "6RcR8GdLAyykPrt4nMUdpytpmVV8uyzDmgDx5T6xzbR7",
					"marketIndex": 1156,
					"name": "MOTHER/USDC",
					"baseTokenIndex": 1156,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "EcZCB4bgV7TQkgsvQeRDJ7N9P5NjYWavv4bTpn3gGDAY",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "AFG3zgZJ2rvwcEFSJSkZWnLETyAES9V13Tz5zvEemJAV",
					"marketIndex": 1158,
					"name": "dualSOL/SOL",
					"baseTokenIndex": 1158,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "F6hzpb2yQfxQwQyZ8dtczeqD2DiEmJujjuq74Qn9a1rX",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "7sBurFvFRUMzWpWyhpc47g7FLKek42LonfGWTtsF4XMg",
					"marketIndex": 1161,
					"name": "digitSOL/SOL",
					"baseTokenIndex": 1161,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "DGK2fx88dMnw6pX88mLcvsqxvybakf6M5HuneM5Jby3i",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9QpUMfD8iGfTzmPs7Kv4Fp2LxeMWRTnZYQF9XZJSW2Ci",
					"marketIndex": 1162,
					"name": "mangoSOL/SOL",
					"baseTokenIndex": 1162,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "65kgETUqPWB4UFFDv5kJLdvGJq8vPwcDk9Z5zBsmfnb8",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "KDzThvteVZDqr5izUD4JEKVsNbzepfeMeViUWhdb7qe",
					"marketIndex": 1163,
					"name": "compassSOL/SOL",
					"baseTokenIndex": 1163,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "G2k1noNu6n2FSL6KjG2UftFTbLk5eUeymeFSG1qMBx3n",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "5wVn7vUtzrDaNSTiWoRs9Vt7yKEZbyNSEJHBYwKi1bTY",
					"marketIndex": 1173,
					"name": "BILLY/USDC",
					"baseTokenIndex": 1173,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BhkScmaQsW7pDpAELazhC2g1FjyqrErWt8HDamPg8wkh",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "9tfBJxzVv5thMCGohjfhXZwUa6FjFzwzXvasjysEtkFL",
					"marketIndex": 1179,
					"name": "USDY/USDC",
					"baseTokenIndex": 1179,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "5CyEuFbohD5UrD1drpyMAMStmj61Tq8RhodPZ3kh15Kt",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "A85NrNd93g28pAeiK9wvRhmffnm1RRVPJJEZ2BKewo2P",
					"marketIndex": 1189,
					"name": "DEAN/USDC",
					"baseTokenIndex": 1189,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "6N713m6QL5UPaPTd84hgXXLjVqwYD3KBHMs429oNKeuY",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "BRbW2tFJz6veXu35xBL4qRFTZH5TqDChqtYXGrk2q6uv",
					"marketIndex": 1199,
					"name": "LNGCAT/USDC",
					"baseTokenIndex": 1199,
					"quoteTokenIndex": 0,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BrYDGJLj11yLYyzyGtkFPGRCGM5MkqHVgreBez876cEa",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "CvbE57gEakLCxbJfM5y6vvUrMLnJ2Urvm9xiWdoDkgjL",
					"marketIndex": 1252,
					"name": "stepSOL/SOL",
					"baseTokenIndex": 1252,
					"quoteTokenIndex": 4,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "BvFA1Y9eQXnGAdTgNXFF7qA63QKKFzTRvW65UcNFp1kc",
					"active": true
				},
				{
					"group": "78b8f4cGCwmZ9ysPFMWLaLTkkaYnUjwMJYStWe5RTSSX",
					"publicKey": "5x9NRYLbxi4DVwF8zEVndkgCHXaDQ3aZWStoh5qD8smC",
					"marketIndex": 1255,
					"name": "STEP/STEPSOL",
					"baseTokenIndex": 916,
					"quoteTokenIndex": 1252,
					"serumProgram": "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX",
					"serumMarketExternal": "7EpKkSLFpeNYHnk1uiewJHURESb9LkhdURGaqPkevmkX",
					"active": true
				}
			],
			"openbookV2Markets": []
		}
*/

/// Return (mints, obv2-markets)
pub async fn fetch_mango_data() -> anyhow::Result<MangoMetadata> {
    let address = "https://api.mngo.cloud/data/v4/group-metadata";
    let http_client = reqwest::Client::new();
    let response = http_client
        .get(address)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .context("mango group request")?;

    let metadata: anyhow::Result<MangoGroupMetadataResponse> =
        crate::utils::http_error_handling(response).await;

    let metadata = metadata?;

    let mut mints = HashSet::new();
    let mut obv2_markets = HashSet::new();

    for group in &metadata.groups {
        for token in &group.tokens {
            mints.insert(Pubkey::from_str(token.mint.as_str())?);
        }
        for market in &group.openbook_v2_markets {
            obv2_markets.insert(Pubkey::from_str(market.serum_market_external.as_str())?);
        }
    }

    Ok(MangoMetadata {
        mints,
        obv2_markets,
    })
}

pub fn spawn_mango_watcher(
    initial_data: &Option<MangoMetadata>,
    _config: &Config,
) -> Option<JoinHandle<()>> {
    if initial_data.is_none() {
        return None;
    }
    let initial_data = initial_data.clone().unwrap();

    Some(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 15));
        interval.tick().await;

        loop {
            interval.tick().await;

            let data = fetch_mango_data().await;
            match data {
                Ok(data) => {
                    info!(
                        tokens = data.mints.len(),
                        obv2_markets = data.obv2_markets.len(),
                        "mango metadata"
                    );

                    if data
                        .obv2_markets
                        .difference(&initial_data.obv2_markets)
                        .count()
                        > 0
                    {
                        warn!("new obv2 markets on mango");
                        break;
                    }
                    if data.mints.difference(&initial_data.mints).count() > 0 {
                        warn!("new tokens on mango");
                        break;
                    }
                }
                Err(e) => {
                    error!("Couldn't fetch mango metadata data: {}", e);
                }
            }
        }
    }))
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MangoGroupMetadataResponse {
    pub groups: Vec<MangoGroupMetadata>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MangoGroupMetadata {
    pub tokens: Vec<MangoGroupTokenMetadata>,
    pub openbook_v2_markets: Vec<MangoGroupObv2MarketsMetadata>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MangoGroupTokenMetadata {
    pub mint: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MangoGroupObv2MarketsMetadata {
    pub serum_market_external: String,
}
