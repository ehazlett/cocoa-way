use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientMetadata {
    pub vm_name: String,
    pub color: Option<[u8; 3]>,
}

static CLIENTS: OnceLock<Mutex<HashMap<u32, ClientMetadata>>> = OnceLock::new();

fn clients() -> &'static Mutex<HashMap<u32, ClientMetadata>> {
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register(pid: u32, vm_name: String, color: Option<[u8; 3]>) {
    clients()
        .lock()
        .expect("client metadata lock poisoned")
        .insert(pid, ClientMetadata { vm_name, color });
}

pub fn get(pid: u32) -> Option<ClientMetadata> {
    let clients = clients().lock().expect("client metadata lock poisoned");
    if let Some(metadata) = clients.get(&pid) {
        return Some(metadata.clone());
    }
    // waypipe opens Wayland connections from client-conn child processes. Veil
    // starts the parent as a process-group leader and registers that PID, while
    // every helper inherits the same process group.
    let process_group = unsafe { libc::getpgid(pid as libc::pid_t) };
    (process_group > 0)
        .then(|| clients.get(&(process_group as u32)).cloned())
        .flatten()
}

pub fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ])
}

#[cfg(target_os = "macos")]
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of_val(&pid) as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut pid as *mut libc::pid_t).cast(),
            &mut len,
        )
    };
    (result == 0 && pid > 0).then_some(pid as u32)
}

#[cfg(not(target_os = "macos"))]
pub fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::parse_hex_color;

    #[test]
    fn parses_normalized_hex_colors() {
        assert_eq!(parse_hex_color("33ff88"), Some([0x33, 0xff, 0x88]));
        assert_eq!(parse_hex_color("#AABBCC"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(parse_hex_color("bad"), None);
        assert_eq!(parse_hex_color("gg0000"), None);
    }
}
