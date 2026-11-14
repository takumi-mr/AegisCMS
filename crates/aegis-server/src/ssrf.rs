//! 外部へ HTTP で出ていくときの SSRF 対策（宛先の IP アドレスの判定と、内部に向く名前解決の遮断）。
//!
//! プラグインの `http_request`（`plugin_host`）とリンク切れ検査（`broken_links`）が共有する

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// 通信の宛先として許す IP アドレスか。ループバック・プライベート・リンクローカル（クラウドの
/// メタデータの `169.254.169.254` を含む）・共有アドレス・マルチキャスト・未指定などは許さない。
/// IPv4 射影の IPv6 アドレス（`::ffff:127.0.0.1`）は、中の IPv4 で判断する
pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // 100.64.0.0/10（キャリア NAT の共有アドレス）
                || (a == 100 && (64..=127).contains(&b))
                // 192.0.0.0/24（IETF プロトコル割り当て）・198.18.0.0/15（ベンチマーク）
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (b == 18 || b == 19))
                // 240.0.0.0/4（予約）
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            let first = seg[0];
            // IPv4 アドレスを中に持つ形は、中の IPv4 で判断する（内部の IPv4 に届く経路になりうる）
            let embedded =
                |a: u16, b: u16| Ipv4Addr::new((a >> 8) as u8, a as u8, (b >> 8) as u8, b as u8);
            // 64:ff9b::/96（NAT64）の下位 32 ビット
            if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                return is_public_ip(IpAddr::V4(embedded(seg[6], seg[7])));
            }
            // 2002::/16（6to4）の第 2・3 ブロック
            if first == 0x2002 {
                return is_public_ip(IpAddr::V4(embedded(seg[1], seg[2])));
            }
            // ::/96（IPv4 互換。廃止された形）の下位 32 ビット（`::` と `::1` は下で弾く）
            if seg[..6] == [0; 6] && (seg[6] != 0 || seg[7] > 1) {
                return is_public_ip(IpAddr::V4(embedded(seg[6], seg[7])));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7（ユニークローカル）・fe80::/10（リンクローカル）
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                // 2001:db8::/32（ドキュメント用）・2001::/32（Teredo。中の IPv4 を隠せる）
                || (first == 0x2001 && (seg[1] == 0x0db8 || seg[1] == 0))
                // 100::/64（廃棄用）・64:ff9b:1::/48（NAT64 のローカル用）
                || (first == 0x100 && seg[1..4] == [0, 0, 0])
                || (first == 0x64 && seg[1] == 0xff9b && seg[2] == 1))
        }
    }
}

/// 名前解決の結果から、公開されたアドレスだけを残す DNS リゾルバー。ホスト名が内部のアドレス
/// （`localhost` や、内部の IP に向けた DNS）に解決されるときは接続しない。接続のたびに
/// 解決した結果で判断するので、確認のあとに名前の向き先を変えられても（DNS リバインディング）通らない
pub(crate) struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|addr| is_public_ip(addr.ip()))
                .collect();
            if addrs.is_empty() {
                let error: Box<dyn std::error::Error + Send + Sync> =
                    format!("'{}' does not resolve to a public IP address", host).into();
                return Err(error);
            }
            let addrs: reqwest::dns::Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "240.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "2001:db8::1",
            // IPv4 を中に持つ形（NAT64・6to4・IPv4 互換・Teredo）で、内部の IPv4 を指すもの
            "64:ff9b::7f00:1",
            "64:ff9b::a00:1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::1",
            "2002:c0a8:101::",
            "::7f00:1",
            "::a00:1",
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
            "100::1",
            "64:ff9b:1::1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{}", ip);
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
            // 公開された IPv4 を中に持つ形
            "64:ff9b::808:808",
            "2002:808:808::1",
            "::808:808",
        ] {
            assert!(is_public_ip(ip.parse().unwrap()), "{}", ip);
        }
    }
}
