#[cfg(not(target_os = "ios"))]
use hbb_common::whoami;
use hbb_common::{
    allow_err,
    anyhow::bail,
    config::Config,
    config::{self, RENDEZVOUS_PORT},
    log,
    protobuf::Message as _,
    rendezvous_proto::*,
    tokio::{
        self,
        sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    },
    ResultType,
};

use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    time::Instant,
};

/// 렌데즈부 프로토콜 메시지 타입 별칭
type Message = RendezvousMessage;

/// LAN 디스커버리 리스너를 시작합니다
/// UDP 브로드캐스트를 수신하여 네트워크 상의 다른 RustDesk 인스턴스를 감지합니다
#[cfg(not(target_os = "ios"))]
pub(super) fn start_listening() -> ResultType<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], get_broadcast_port()));
    let socket = std::net::UdpSocket::bind(addr)?;
    socket.set_read_timeout(Some(std::time::Duration::from_millis(1000)))?;
    log::info!("lan discovery listener started");

    // LAN 디스커버리 ping 메시지를 수신하고 pong으로 응답
    loop {
        let mut buf = [0; 2048];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            if let Ok(msg_in) = Message::parse_from_bytes(&buf[0..len]) {
                match msg_in.union {
                    Some(rendezvous_message::Union::PeerDiscovery(p)) => {
                        if p.cmd == "ping"
                            && config::option2bool(
                                "enable-lan-discovery",
                                &Config::get_option("enable-lan-discovery"),
                            )
                        {
                            let id = Config::get_id();
                            // 자신이 보낸 ping은 무시
                            if p.id == id {
                                continue;
                            }
                            if let Some(self_addr) = get_ipaddr_by_peer(&addr) {
                                let mut msg_out = Message::new();
                                let mut hostname = crate::whoami_hostname();
                                // 기본 호스트명 "localhost"는 혼동하기 쉬우므로 "unknown"으로 변경
                                if hostname == "localhost" {
                                    hostname = "unknown".to_owned();
                                }
                                let peer = PeerDiscovery {
                                    cmd: "pong".to_owned(),
                                    mac: get_mac(&self_addr),
                                    id,
                                    hostname,
                                    username: crate::platform::get_active_username(),
                                    platform: whoami::platform().to_string(),
                                    ..Default::default()
                                };
                                msg_out.set_peer_discovery(peer);
                                socket.send_to(&msg_out.write_to_bytes()?, addr).ok();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// LAN 디스커버리를 실행합니다
/// 네트워크 상의 다른 RustDesk 인스턴스를 찾기 위해 ping을 보내고 응답을 수집합니다
#[tokio::main(flavor = "current_thread")]
pub async fn discover() -> ResultType<()> {
    let sockets = send_query()?;
    let rx = spawn_wait_responses(sockets);
    handle_received_peers(rx).await?;

    log::info!("discover ping done");
    Ok(())
}

/// Wake-on-LAN (WoL) 신호를 전송하여 오프라인 피어를 깨웁니다
/// id에 해당하는 피어의 MAC 주소를 찾아 모든 네트워크 인터페이스에서 WoL 신호를 송신합니다
pub fn send_wol(id: String) {
    let interfaces = default_net::get_interfaces();
    for peer in &config::LanPeers::load().peers {
        if peer.id == id {
            for (_, mac) in peer.ip_mac.iter() {
                if let Ok(mac_addr) = mac.parse() {
                    for interface in &interfaces {
                        for ipv4 in &interface.ipv4 {
                            // 마스크 확인을 제거하여 예상치 못한 버그 회피
                            // if (u32::from(ipv4.addr) & u32::from(ipv4.netmask)) == (u32::from(peer_ip) & u32::from(ipv4.netmask))
                            log::info!("Send wol to {mac_addr} of {}", ipv4.addr);
                            allow_err!(wol::send_wol(mac_addr, None, Some(IpAddr::V4(ipv4.addr))));
                        }
                    }
                }
            }
            break;
        }
    }
}

/// LAN 디스커버리 브로드캐스트 포트를 반환합니다
#[inline]
fn get_broadcast_port() -> u16 {
    (RENDEZVOUS_PORT + 3) as _
}

/// IP 주소에 해당하는 MAC 주소를 반환합니다
fn get_mac(_ip: &IpAddr) -> String {
    #[cfg(not(target_os = "ios"))]
    if let Ok(mac) = get_mac_by_ip(_ip) {
        mac.to_string()
    } else {
        "".to_owned()
    }
    #[cfg(target_os = "ios")]
    "".to_owned()
}

/// 주어진 IP 주소의 MAC 주소를 조회합니다
#[cfg(not(target_os = "ios"))]
fn get_mac_by_ip(ip: &IpAddr) -> ResultType<String> {
    for interface in default_net::get_interfaces() {
        match ip {
            IpAddr::V4(local_ipv4) => {
                if interface.ipv4.iter().any(|x| x.addr == *local_ipv4) {
                    if let Some(mac_addr) = interface.mac_addr {
                        return Ok(mac_addr.address());
                    }
                }
            }
            IpAddr::V6(local_ipv6) => {
                if interface.ipv6.iter().any(|x| x.addr == *local_ipv6) {
                    if let Some(mac_addr) = interface.mac_addr {
                        return Ok(mac_addr.address());
                    }
                }
            }
        }
    }
    bail!("No interface found for ip: {:?}", ip);
}

/// 주어진 peer 주소로 연결할 때 사용할 로컬 IP 주소를 결정합니다
/// 주로 https://github.com/shellrow/default-net/blob/cf7ca24e7e6e8e566ed32346c9cfddab3f47e2d6/src/interface/shared.rs#L4 참조
fn get_ipaddr_by_peer<A: ToSocketAddrs>(peer: A) -> Option<IpAddr> {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };

    match socket.connect(peer) {
        Ok(()) => (),
        Err(_) => return None,
    };

    match socket.local_addr() {
        Ok(addr) => return Some(addr.ip()),
        Err(_) => return None,
    };
}

/// LAN 디스커버리용 UDP 브로드캐스트 소켓을 생성합니다
/// 모든 로컬 IPv4 주소에서 브로드캐스트를 수신할 수 있도록 소켓을 만듭니다
fn create_broadcast_sockets() -> Vec<UdpSocket> {
    let mut ipv4s = Vec::new();
    // TODO: IPv4 주소를 가져오는 더 나은 방법을 사용할 수도 있습니다.
    // 현재는 디스커버리를 위해 `[Ipv4Addr::UNSPECIFIED]`를 사용해도 괜찮습니다.
    // iOS 시뮬레이터 x86_64에서 flutter build 시
    // `default_net::get_interfaces()`가 정의되지 않은 심볼 에러를 일으킵니다
    #[cfg(not(any(target_os = "ios")))]
    for interface in default_net::get_interfaces() {
        for ipv4 in &interface.ipv4 {
            ipv4s.push(ipv4.addr.clone());
        }
    }
    ipv4s.push(Ipv4Addr::UNSPECIFIED); // 안정성을 위해 추가
    let mut sockets = Vec::new();
    for v4_addr in ipv4s {
        // v4_addr.is_private() 확인 제거: https://github.com/rustdesk/rustdesk/issues/4663
        if let Ok(s) = UdpSocket::bind(SocketAddr::from((v4_addr, 0))) {
            if s.set_broadcast(true).is_ok() {
                sockets.push(s);
            }
        }
    }
    sockets
}

/// LAN 디스커버리 ping 메시지를 전송합니다
/// 모든 로컬 인터페이스에서 브로드캐스트 ping을 보냅니다
fn send_query() -> ResultType<Vec<UdpSocket>> {
    let sockets = create_broadcast_sockets();
    if sockets.is_empty() {
        bail!("Found no bindable ipv4 addresses");
    }

    let mut msg_out = Message::new();
    // 모바일 플랫폼에서는 MAC 주소를 가져올 수 없을 수 있으므로
    // ID를 사용하여 자신의 ping을 감지하지 않도록 합니다
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let id = crate::ui_interface::get_id();
    // `crate::ui_interface::get_id()` 호출 시 에러 발생:
    // `get_id()`는 async 코드를 사용하므로 `current_thread`에서 허용되지 않습니다.
    // 데스크톱 플랫폼에서는 ID를 가져올 필요 없습니다.
    // MAC 주소를 사용하여 장치를 식별할 수 있습니다.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let id = "".to_owned();
    let peer = PeerDiscovery {
        cmd: "ping".to_owned(),
        id,
        ..Default::default()
    };
    msg_out.set_peer_discovery(peer);
    let out = msg_out.write_to_bytes()?;
    let maddr = SocketAddr::from(([255, 255, 255, 255], get_broadcast_port()));
    for socket in &sockets {
        allow_err!(socket.send_to(&out, maddr));
    }
    log::info!("discover ping sent");
    Ok(sockets)
}

/// 주어진 소켓에서 ping 응답(pong)을 기다리고 발견된 피어를 채널로 전송합니다
/// timeout: 응답 대기 시간
/// tx: 발견된 피어를 전송할 채널
fn wait_response(
    socket: UdpSocket,
    timeout: Option<std::time::Duration>,
    tx: UnboundedSender<config::DiscoveryPeer>,
) -> ResultType<()> {
    let mut last_recv_time = Instant::now();

    let local_addr = socket.local_addr();
    let try_get_ip_by_peer = match local_addr.as_ref() {
        Err(..) => true,
        Ok(addr) => addr.ip().is_unspecified(),
    };
    let mut mac: Option<String> = None;

    socket.set_read_timeout(timeout)?;

    // ping 응답을 받을 때까지 루프
    loop {
        let mut buf = [0; 2048];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            if let Ok(msg_in) = Message::parse_from_bytes(&buf[0..len]) {
                match msg_in.union {
                    Some(rendezvous_message::Union::PeerDiscovery(p)) => {
                        last_recv_time = Instant::now();
                        if p.cmd == "pong" {
                            // 로컬 MAC 주소 결정
                            let local_mac = if try_get_ip_by_peer {
                                if let Some(self_addr) = get_ipaddr_by_peer(&addr) {
                                    get_mac(&self_addr)
                                } else {
                                    "".to_owned()
                                }
                            } else {
                                match mac.as_ref() {
                                    Some(m) => m.clone(),
                                    None => {
                                        let m = if let Ok(local_addr) = local_addr {
                                            get_mac(&local_addr.ip())
                                        } else {
                                            "".to_owned()
                                        };
                                        mac = Some(m.clone());
                                        m
                                    }
                                }
                            };

                            // 다른 피어인 경우에만 전송 (자신이 아닌 경우)
                            if local_mac.is_empty() && p.mac.is_empty() || local_mac != p.mac {
                                allow_err!(tx.send(config::DiscoveryPeer {
                                    id: p.id.clone(),
                                    ip_mac: HashMap::from([
                                        (addr.ip().to_string(), p.mac.clone(),)
                                    ]),
                                    username: p.username.clone(),
                                    hostname: p.hostname.clone(),
                                    platform: p.platform.clone(),
                                    online: true,
                                }));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // 3초 동안 응답이 없으면 종료
        if last_recv_time.elapsed().as_millis() > 3_000 {
            break;
        }
    }
    Ok(())
}

/// 여러 소켓에서 ping 응답을 대기하는 스레드를 생성합니다
/// 각 소켓마다 별도의 스레드를 생성하여 응답을 수집합니다
fn spawn_wait_responses(sockets: Vec<UdpSocket>) -> UnboundedReceiver<config::DiscoveryPeer> {
    let (tx, rx) = unbounded_channel::<_>();
    for socket in sockets {
        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            allow_err!(wait_response(
                socket,
                Some(std::time::Duration::from_millis(10)),
                tx_clone
            ));
        });
    }
    rx
}

/// 발견된 피어들을 처리하고 LAN 피어 목록을 업데이트합니다
/// 수신 채널에서 피어를 받아 설정에 저장하고 필요시 UI에 알립니다
async fn handle_received_peers(mut rx: UnboundedReceiver<config::DiscoveryPeer>) -> ResultType<()> {
    let mut peers = config::LanPeers::load().peers;
    // 모든 피어를 오프라인으로 표시 (새로 발견된 것만 온라인으로 변경됨)
    peers.iter_mut().for_each(|peer| {
        peer.online = false;
    });

    let mut response_set = HashSet::new();
    let mut last_write_time: Option<Instant> = None;
    loop {
        tokio::select! {
            data = rx.recv() => match data {
                Some(mut peer) => {
                    // 같은 피어의 응답이 중복되었는지 확인
                    let in_response_set = !response_set.insert(peer.id.clone());
                    if let Some(pos) = peers.iter().position(|x| x.is_same_peer(&peer) ) {
                        let peer1 = peers.remove(pos);
                        if in_response_set {
                            // 중복 응답인 경우 IP-MAC 매핑을 확장
                            peer.ip_mac.extend(peer1.ip_mac);
                            peer.online = true;
                        }
                    }
                    peers.insert(0, peer);
                    // 300ms 이상 경과 시 설정에 저장 (I/O 빈도 줄이기)
                    if last_write_time.map(|t| t.elapsed().as_millis() > 300).unwrap_or(true)  {
                        config::LanPeers::store(&peers);
                        #[cfg(feature = "flutter")]
                        crate::flutter_ffi::main_load_lan_peers();
                        last_write_time = Some(Instant::now());
                    }
                }
                None => {
                    break
                }
            }
        }
    }

    // 최종 피어 목록 저장
    config::LanPeers::store(&peers);
    #[cfg(feature = "flutter")]
    crate::flutter_ffi::main_load_lan_peers();
    Ok(())
}
