//! Hardening report generation from discovered devices + stored audit results.

use crate::audit::AuditResult;
use crate::model::Device;

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_time(ms: i64) -> String {
    if ms <= 0 {
        return "-".into();
    }
    // Keep it simple / portable: epoch millis as ISO-ish UTC without chrono crate.
    let secs = ms / 1000;
    let days = secs / 86400;
    let rem = secs % 86400;
    let hours = rem / 3600;
    let mins = (rem % 3600) / 60;
    let s = rem % 60;
    // Approximate calendar from Unix epoch is heavy; show epoch + HMS of day.
    format!("unix+{days}d {hours:02}:{mins:02}:{s:02} UTC")
}

pub fn render_markdown(devices: &[Device], audits: &[AuditResult]) -> String {
    let mut out = String::new();
    out.push_str("# 摄像头安全加固报告\n\n");
    out.push_str(&format!(
        "生成时间：{}  \n设备数：{}  \n已审计：{}\n\n",
        format_time(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        ),
        devices.len(),
        audits.len()
    ));
    out.push_str("> 本报告仅基于授权范围内的发现与指纹比对结果，用于安全自查与加固，不包含漏洞利用步骤。\n\n");

    out.push_str("## 1. 发现设备一览\n\n");
    if devices.is_empty() {
        out.push_str("（暂无设备记录）\n\n");
    } else {
        out.push_str("| IP | 端口 | 协议 | 来源 | Server |\n| --- | --- | --- | --- | --- |\n");
        for d in devices {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                d.ip,
                d.open_ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                d.protocols.join(" "),
                d.source,
                d.http_server.as_deref().unwrap_or("-").replace('|', "/")
            ));
        }
        out.push('\n');
    }

    out.push_str("## 2. 审计与风险\n\n");
    if audits.is_empty() {
        out.push_str("（暂无审计结果。请在发现页对设备执行「审计」。）\n\n");
    } else {
        for a in audits {
            out.push_str(&format!("### {}\n\n", a.ip));
            out.push_str(&format!(
                "- 厂商：{}  \n- HTTP 端口：{}  \n- Server：{}  \n- Realm：{}\n\n",
                a.vendor.as_deref().unwrap_or("未知"),
                a.http_port
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".into()),
                a.http_server.as_deref().unwrap_or("-"),
                a.realm.as_deref().unwrap_or("-")
            ));

            if !a.weak_credentials.is_empty() {
                out.push_str("**弱口令命中：**\n\n");
                for c in &a.weak_credentials {
                    out.push_str(&format!(
                        "- `{}` `{}:{}` @ {}\n",
                        c.scheme,
                        c.username,
                        if c.password.is_empty() {
                            "(空)"
                        } else {
                            &c.password
                        },
                        c.path
                    ));
                }
                out.push('\n');
            }

            if a.findings.is_empty() {
                out.push_str("风险条目：无匹配规则。\n\n");
            } else {
                out.push_str("| 严重级 | CVE | 标题 | 加固建议 |\n| --- | --- | --- | --- |\n");
                for f in &a.findings {
                    out.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        f.severity,
                        f.cve,
                        f.title.replace('|', "/"),
                        f.advice.replace('|', "/")
                    ));
                }
                out.push('\n');
            }

            if !a.notes.is_empty() {
                out.push_str("备注：\n\n");
                for n in &a.notes {
                    out.push_str(&format!("- {}\n", n));
                }
                out.push('\n');
            }
        }
    }

    out.push_str("## 3. 通用加固清单\n\n");
    out.push_str(
        "1. 修改所有默认/弱口令，启用失败锁定。  \n\
         2. 关闭不必要的公网端口映射与 UPnP。  \n\
         3. 升级到厂商已修复相关 CVE 的固件版本。  \n\
         4. 将摄像头置于独立 VLAN / 隔离网段。  \n\
         5. 管理口仅允许运维主机访问。  \n\
         6. 定期复查本工具发现结果与审计日志。\n",
    );
    out
}

pub fn render_html(devices: &[Device], audits: &[AuditResult]) -> String {
    let md_like = render_markdown(devices, audits);
    // Lightweight HTML wrapper; keep content as preformatted markdown-ish sections.
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<title>摄像头安全加固报告</title>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 32px; color: #111; line-height: 1.5; }}
h1,h2,h3 {{ margin-top: 1.4em; }}
table {{ border-collapse: collapse; width: 100%; margin: 12px 0; }}
th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; font-size: 14px; }}
th {{ background: #f5f5f5; }}
.note {{ color: #555; background: #f8fafc; padding: 12px; border-radius: 8px; }}
code {{ background: #f3f4f6; padding: 1px 4px; border-radius: 4px; }}
</style>
</head>
<body>
<pre style="white-space: pre-wrap; font-family: inherit;">{}</pre>
</body>
</html>
"#,
        esc_html(&md_like)
    )
}
