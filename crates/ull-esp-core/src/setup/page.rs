//! The setup page. Plain HTML with a little CSS, sized for a phone.

use core::fmt::{self, Write};

use crate::settings::{SSID_MAX, Settings, Text};

/// A network found by the scan.
#[derive(Clone, Copy, Debug)]
pub struct Network {
    pub ssid: Text<SSID_MAX>,
    pub rssi: i8,
    pub open: bool,
}

pub struct FormView<'a> {
    /// Shown in the title, e.g. `sht3x-ssd1306 9c9c1fe9dda8`.
    pub device: &'a str,
    pub firmware: &'a str,
    /// Prefills the fields; the password is never sent back.
    pub current: &'a Settings,
    pub networks: &'a [Network],
    /// Why the last save was refused.
    pub error: Option<&'a str>,
}

const STYLE: &str = "\
:root{color-scheme:light dark;--bg:#f6f6f4;--card:#fff;--ink:#1b1b1a;--muted:#62625e;--line:#d9d9d4;--accent:#1f5fbf;--bad:#b3261e}\
@media(prefers-color-scheme:dark){:root{--bg:#141413;--card:#1f1f1e;--ink:#ececea;--muted:#a3a39e;--line:#3a3a37;--accent:#7aa7ff;--bad:#ff8a80}}\
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.4 system-ui,-apple-system,sans-serif}\
main{max-width:420px;margin:0 auto;padding:16px}h1{font-size:20px;margin:8px 0 4px}h2{font-size:15px;margin:20px 0 8px;color:var(--muted);font-weight:600}\
p{margin:0 0 12px}.muted{color:var(--muted);font-size:14px}.error{color:var(--bad)}\
.list{background:var(--card);border:1px solid var(--line);border-radius:10px;overflow:hidden}\
.list button{display:flex;width:100%;justify-content:space-between;gap:12px;min-height:48px;padding:12px 14px;border:0;border-top:1px solid var(--line);background:none;color:inherit;font:inherit;text-align:left}\
.list button:first-child{border-top:0}.list span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.list small{color:var(--muted);flex:none}\
label{display:block;margin-top:14px;font-weight:600;font-size:15px}label small{display:block;font-weight:400;color:var(--muted)}\
input{display:block;width:100%;margin-top:6px;min-height:48px;padding:10px 12px;border:1px solid var(--line);border-radius:10px;background:var(--card);color:inherit;font:inherit}\
.save{display:block;width:100%;margin-top:22px;min-height:50px;border:0;border-radius:10px;background:var(--accent);color:#fff;font:inherit;font-weight:600}\
@media(prefers-color-scheme:dark){.save{color:#10141c}}";

fn head(out: &mut impl Write, title: &str) -> fmt::Result {
    write!(
        out,
        "<!doctype html><html lang=en><meta charset=utf-8>\
         <meta name=viewport content=\"width=device-width,initial-scale=1\">\
         <title>{}</title><style>{STYLE}</style><main>",
        Escaped(title)
    )
}

/// The form: networks nearby, Wi-Fi name and password, fleet server.
pub fn render_form(out: &mut impl Write, view: &FormView<'_>) -> fmt::Result {
    head(out, "Device setup")?;
    write!(
        out,
        "<h1>Device setup</h1><p class=muted>{} · {}</p>",
        Escaped(view.device),
        Escaped(view.firmware)
    )?;
    if let Some(error) = view.error {
        write!(out, "<p class=error>{}</p>", Escaped(error))?;
    }

    out.write_str("<h2>Networks nearby</h2>")?;
    if view.networks.is_empty() {
        out.write_str("<p class=muted>None found. Type the name below.</p>")?;
    } else {
        out.write_str("<div class=list>")?;
        for network in view.networks {
            let ssid = Escaped(network.ssid.as_str());
            write!(
                out,
                "<button type=button data-ssid=\"{ssid}\"><span>{ssid}</span><small>{}{}</small></button>",
                signal(network.rssi),
                if network.open { ", open" } else { "" }
            )?;
        }
        out.write_str("</div>")?;
    }

    let current = view.current;
    let password_hint = if current.wifi_password.is_empty() {
        "Leave empty for an open network."
    } else {
        "Leave empty to keep the saved password."
    };
    write!(
        out,
        "<form method=post action=/save>\
         <label>Wi-Fi name<input id=ssid name=ssid required maxlength={SSID_MAX} \
         autocapitalize=off autocorrect=off spellcheck=false value=\"{}\"></label>\
         <label>Wi-Fi password<small>{password_hint}</small>\
         <input name=password type=password maxlength=64 autocomplete=off></label>\
         <label>Fleet server<small>Leave empty if there is none.</small>\
         <input name=server inputmode=url autocapitalize=off autocorrect=off spellcheck=false \
         placeholder=\"http://192.168.1.10:3000\" value=\"{}\"></label>\
         <button class=save>Save and restart</button></form>",
        Escaped(current.wifi_ssid.as_str()),
        Escaped(current.server_url.as_str()),
    )?;
    out.write_str(
        "<script>for(const b of document.querySelectorAll('[data-ssid]'))\
         b.onclick=()=>{const i=document.getElementById('ssid');i.value=b.dataset.ssid;\
         document.querySelector('[name=password]').focus()}</script></main>",
    )
}

/// After a successful save, just before the restart.
pub fn render_saved(out: &mut impl Write, ssid: &str) -> fmt::Result {
    head(out, "Saved")?;
    write!(
        out,
        "<h1>Saved</h1><p>The device restarts and joins <b>{}</b>.</p>\
         <p class=muted>You can close this page and switch your phone back to your usual Wi-Fi. \
         If the device cannot join, it opens this setup network again within 5 minutes.</p></main>",
        Escaped(ssid)
    )
}

fn signal(rssi: i8) -> &'static str {
    match rssi {
        -60.. => "strong",
        -72..=-61 => "good",
        _ => "weak",
    }
}

/// Writes text with `& < > " '` escaped for HTML text and quoted attributes.
pub struct Escaped<'a>(pub &'a str);

impl fmt::Display for Escaped<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut rest = self.0;
        while let Some(index) = rest.find(['&', '<', '>', '"', '\'']) {
            f.write_str(&rest[..index])?;
            f.write_str(match rest.as_bytes()[index] {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                b'"' => "&quot;",
                _ => "&#39;",
            })?;
            rest = &rest[index + 1..];
        }
        f.write_str(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_escapes_names_and_never_echoes_the_password() {
        let current = Settings::new("old\"net", "secret-password", "http://10.0.0.2:3000").unwrap();
        let networks = [Network {
            ssid: Text::new("<b>cafe</b>").unwrap(),
            rssi: -50,
            open: true,
        }];
        let mut html = String::new();
        render_form(
            &mut html,
            &FormView {
                device: "dev 1",
                firmware: "0.1.0",
                current: &current,
                networks: &networks,
                error: Some("Enter the Wi-Fi name."),
            },
        )
        .unwrap();
        assert!(html.contains("&lt;b&gt;cafe&lt;/b&gt;"));
        assert!(html.contains("value=\"old&quot;net\""));
        assert!(html.contains("strong, open"));
        assert!(html.contains("keep the saved password"));
        assert!(!html.contains("secret-password"));
        assert!(!html.contains("<b>cafe"));
    }

    #[test]
    fn saved_page_names_the_network() {
        let mut html = String::new();
        render_saved(&mut html, "home & co").unwrap();
        assert!(html.contains("<b>home &amp; co</b>"));
    }
}
