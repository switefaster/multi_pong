use multi_pong::network::udp;
use serde::{Deserialize, Serialize};
use tokio::{
    net::UdpSocket,
    stream::StreamExt,
    time::Duration,
    time::{delay_for, Instant},
    join,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Point(u32, u128);

#[tokio::main]
async fn main() {
    println!("UDP Example");

    // 100ms
    let timeout = Duration::new(0, 1_000_000);

    let socket1 = UdpSocket::bind("0.0.0.0:4001").await.unwrap();
    socket1.connect("0.0.0.0:4002").await.unwrap();
    let socket2 = UdpSocket::bind("0.0.0.0:4002").await.unwrap();
    socket2.connect("0.0.0.0:4001").await.unwrap();
    let (send1, _) = udp::start_udp_loop::<Point>(socket1, timeout, 10, 20);
    let (_, mut recv2) = udp::start_udp_loop::<Point>(socket2, timeout, 10, 20);

    let start = Instant::now();

    let recv_task = tokio::spawn(async move {
        let mut reliable_packets = 0;
        loop {
            let p = recv2.next().await.unwrap();
            if p.slot != 0 {
                reliable_packets += 1;
                println!("Reliable packets: {}", reliable_packets);
            }
            println!(
                "Delay for {} in {}: {}",
                p.data.0,
                p.slot,
                start.elapsed().as_micros() - p.data.1
            );
        }
    });

    let send_task = tokio::spawn(async move {
        for i in 0..100 {
            let packet = if i % 5 == 0 {
                udp::PacketType::Reliable(Point(i, start.elapsed().as_micros()))
            } else {
                udp::PacketType::Unreliable(Point(i, start.elapsed().as_micros()))
            };
            send1.unbounded_send(packet).unwrap();
            if i % 20 == 0 {
                delay_for(Duration::new(0, 1_000_000)).await;
            }
        }
        // if no delay, the channel would be closed, and the send would terminate immediately
        delay_for(Duration::new(10000, 0)).await;
        send1.close_channel();
    });

    let _ = join!(send_task, recv_task);
}
