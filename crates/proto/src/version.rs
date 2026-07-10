//! RPC 协议版本控制。
//!
//! 定义协议版本常量与协商逻辑，用于 Thrift 消息版本协商。
//!
//! ## 版本协商流程
//! 1. 客户端在 `msg` 中发送 `protocol_version`（Thrift optional 字段）
//! 2. 服务端检查版本是否在支持范围内
//! 3. 返回协商结果版本或错误
//! 4. 旧客户端不发送 `protocol_version` → 服务端默认使用 version 1（零破坏）

/// 当前协议版本。
pub const CURRENT_PROTOCOL_VERSION: i32 = 2;

/// 最低支持的协议版本。
pub const MIN_SUPPORTED_VERSION: i32 = 1;

/// 协议版本协商结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionNegotiation {
    /// 协商成功，使用指定版本
    Accepted(i32),
    /// 客户端版本过低，不支持
    ClientTooOld { client_version: i32, min_supported: i32 },
    /// 客户端版本过高，服务端需要升级
    ServerTooOld { client_version: i32, server_version: i32 },
}

/// 检查客户端版本是否在服务端支持范围内。
///
/// # Arguments
/// * `client_version` - 客户端发送的协议版本（`None` 表示旧客户端，默认 version 1）
///
/// # Returns
/// * `VersionNegotiation::Accepted(version)` - 协商成功
/// * `VersionNegotiation::ClientTooOld` - 客户端版本过低
/// * `VersionNegotiation::ServerTooOld` - 客户端版本过高（服务端需要升级）
pub fn check_version(client_version: Option<i32>) -> VersionNegotiation {
    let version = client_version.unwrap_or(1); // 旧客户端默认 version 1

    if version < MIN_SUPPORTED_VERSION {
        VersionNegotiation::ClientTooOld {
            client_version: version,
            min_supported: MIN_SUPPORTED_VERSION,
        }
    } else if version > CURRENT_PROTOCOL_VERSION {
        VersionNegotiation::ServerTooOld {
            client_version: version,
            server_version: CURRENT_PROTOCOL_VERSION,
        }
    } else {
        VersionNegotiation::Accepted(version)
    }
}

/// 协商后的有效版本（总是返回服务端和客户端的较小值）。
pub fn negotiated_version(client_version: Option<i32>) -> i32 {
    let cv = client_version.unwrap_or(1);
    cv.min(CURRENT_PROTOCOL_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_client_defaults_to_v1() {
        assert_eq!(check_version(None), VersionNegotiation::Accepted(1));
    }

    #[test]
    fn matching_version_accepted() {
        assert_eq!(
            check_version(Some(CURRENT_PROTOCOL_VERSION)),
            VersionNegotiation::Accepted(CURRENT_PROTOCOL_VERSION)
        );
    }

    #[test]
    fn client_too_old_rejected() {
        assert_eq!(
            check_version(Some(0)),
            VersionNegotiation::ClientTooOld {
                client_version: 0,
                min_supported: 1
            }
        );
    }

    #[test]
    fn server_too_old_detected() {
        assert_eq!(
            check_version(Some(999)),
            VersionNegotiation::ServerTooOld {
                client_version: 999,
                server_version: CURRENT_PROTOCOL_VERSION
            }
        );
    }

    #[test]
    fn negotiated_version_uses_min() {
        assert_eq!(negotiated_version(None), 1);
        assert_eq!(negotiated_version(Some(2)), 2);
        assert_eq!(negotiated_version(Some(10)), CURRENT_PROTOCOL_VERSION);
    }
}
