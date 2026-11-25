//! 僅供隔離測試使用的 W3C WebDriver 與子程序生命週期工具。
use base64::Engine;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};

pub fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("必要測試環境未設定：{name}"))
}

pub struct Process(pub Child);
impl Process {
    pub fn spawn(binary: &str, args: &[&str], env: &[(&str, String)], log: &Path) -> Self {
        if let Some(parent) = log.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let file = File::create(log).unwrap();
        let mut command = Command::new(binary);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command
            .args(args)
            .envs(env.iter().map(|(k, v)| (*k, v)))
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .unwrap();
        Self(child)
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-TERM", "--", &format!("-{}", self.0.id())])
                .status();
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn wait<T>(label: &str, mut operation: impl FnMut() -> Option<T>) -> T {
    eprintln!("核對：{label}");
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(value) = operation() {
            return value;
        }
        assert!(Instant::now() < until, "等待逾時：{label}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 固定連線至本機測試 driver；不接收任意遠端主機或真實憑證。
pub fn request(port: u16, method: &str, path: &str, payload: Value) -> Result<Value, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(35)))
        .map_err(|e| e.to_string())?;
    let body = payload.to_string();
    write!(stream,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",body.len()).map_err(|e|e.to_string())?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]);
            let length = header.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            });
            if length.is_some_and(|n| bytes.len() >= end + 4 + n) {
                break;
            }
            if header
                .to_ascii_lowercase()
                .contains("transfer-encoding: chunked")
                && bytes.ends_with(b"0\r\n\r\n")
            {
                break;
            }
        }
    }
    let split = bytes
        .windows(4)
        .position(|x| x == b"\r\n\r\n")
        .ok_or("driver回應缺少標頭")?;
    let header = String::from_utf8_lossy(&bytes[..split]);
    let mut body = bytes[split + 4..].to_vec();
    if header
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut decoded = Vec::new();
        let mut i = 0;
        loop {
            let end = body[i..]
                .windows(2)
                .position(|x| x == b"\r\n")
                .ok_or("chunk標頭不完整")?
                + i;
            let len = usize::from_str_radix(
                std::str::from_utf8(&body[i..end])
                    .map_err(|e| e.to_string())?
                    .split(';')
                    .next()
                    .unwrap(),
                16,
            )
            .map_err(|e| e.to_string())?;
            if len == 0 {
                break;
            }
            i = end + 2;
            let part = body.get(i..i + len).ok_or("chunk內容不完整")?;
            decoded.extend_from_slice(part);
            i += len + 2;
        }
        body = decoded;
    }
    let result: Value =
        serde_json::from_slice(&body).map_err(|e| format!("driver JSON錯誤：{e}"))?;
    if !header.starts_with("HTTP/1.1 2") && !header.starts_with("HTTP/1.0 2") {
        return Err(result.to_string());
    }
    let value = result.get("value").cloned().unwrap_or(result);
    if value.get("error").is_some() && value.get("message").is_some() {
        return Err(value.to_string());
    }
    Ok(value)
}

pub struct Browser {
    port: u16,
    id: String,
    artifacts: PathBuf,
    _driver: Process,
}
impl Browser {
    pub fn start(case: &str, native: Option<&str>) -> Self {
        Self::start_with_env(case, native, &[])
    }
    pub fn start_with_env(case: &str, native: Option<&str>, env: &[(&str, String)]) -> Self {
        let artifacts = PathBuf::from(required("RING_ARTIFACTS")).join(case);
        fs::create_dir_all(&artifacts).unwrap();
        let (port, binary, args) = if native.is_some() {
            (
                4444,
                required("RING_TAURI_DRIVER"),
                vec!["--native-driver", "/usr/bin/WebKitWebDriver"],
            )
        } else {
            (9515, required("RING_BROWSER_DRIVER"), vec!["--port=9515"])
        };
        let driver = Process::spawn(&binary, &args, env, &artifacts.join("driver.log"));
        wait("WebDriver啟動", || {
            request(port, "GET", "/status", json!({})).ok()
        });
        let capabilities = match native {
            Some(path) => json!({"browserName":"wry","tauri:options":{"application":path}}),
            None => {
                json!({"browserName":"chrome","goog:chromeOptions":{"binary":required("RING_CHROME_BINARY"),"args":["--headless=new","--no-sandbox","--disable-dev-shm-usage"]}})
            }
        };
        let session = request(
            port,
            "POST",
            "/session",
            json!({"capabilities":{"alwaysMatch":capabilities}}),
        )
        .unwrap();
        let id = session["sessionId"]
            .as_str()
            .expect("driver未回傳sessionId")
            .to_owned();
        Self {
            port,
            id,
            artifacts,
            _driver: driver,
        }
    }
    pub fn call(&self, method: &str, path: &str, args: Value) -> Result<Value, String> {
        request(
            self.port,
            method,
            &format!("/session/{}{path}", self.id),
            args,
        )
    }
    pub fn eval(&self, script: &str, args: Value) -> Value {
        self.call(
            "POST",
            "/execute/sync",
            json!({"script":script,"args":args}),
        )
        .unwrap_or_else(|e| panic!("WebDriver script: {e}; {script}"))
    }
    /// 僅重試狀態讀回；呼叫端不得傳入會修改資料的腳本。
    pub fn read(&self, script: &str, args: Value) -> Value {
        wait("WebDriver狀態讀回", || {
            self.call(
                "POST",
                "/execute/sync",
                json!({"script":script,"args":args}),
            )
            .ok()
        })
    }
    /// 操作只送一次；傳輸回執中斷後，由案例的結果斷言確認，不重送動作。
    pub fn act_once(&self, script: &str, args: Value) {
        if let Err(error) = self.call(
            "POST",
            "/execute/sync",
            json!({"script":script,"args":args}),
        ) {
            if error == "driver回應缺少標頭" || error.contains("Connection reset") {
                eprintln!("操作回執中斷，未重送；後續必須確認結果：{error}");
            } else {
                panic!("WebDriver操作失敗：{error}");
            }
        }
    }
    pub fn go(&self, url: &str) {
        self.call("POST", "/url", json!({"url":url})).unwrap();
    }
    pub fn until(&self, label: &str, script: &str) {
        wait(label, || {
            self.call("POST", "/execute/sync", json!({"script":script,"args":[]}))
                .ok()
                .filter(|x| x == &json!(true))
        });
    }
    pub fn click(&self, selector: &str) {
        self.until(
            "按鈕可操作",
            &format!(
                "const n=document.querySelector({});return !!n&&!n.disabled;",
                json!(selector)
            ),
        );
        self.act_once(
            "document.querySelector(arguments[0]).click();",
            json!([selector]),
        );
    }
    pub fn fill(&self, selector: &str, value: &str) {
        self.act_once("const n=document.querySelector(arguments[0]);n.value=arguments[1];n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}));",json!([selector,value]));
        let actual = self.read(
            "return document.querySelector(arguments[0]).value;",
            json!([selector]),
        );
        assert_eq!(actual, json!(value));
    }
    pub fn save(&self, label: &str) {
        if let Ok(log) = self.call("POST", "/log", json!({"type":"browser"})) {
            let _ = fs::write(
                self.artifacts.join(format!("{label}-console.json")),
                log.to_string(),
            );
        }
        if let Ok(html) = self.call(
            "POST",
            "/execute/sync",
            json!({"script":"return document.documentElement.outerHTML;","args":[]}),
        ) {
            let _ = fs::write(
                self.artifacts.join(format!("{label}.html")),
                html.as_str().unwrap_or(""),
            );
        }
        if let Ok(value) = self.call("GET", "/screenshot", json!({})) {
            if let Some(text) = value.as_str() {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(text) {
                    let _ = fs::write(self.artifacts.join(format!("{label}.png")), bytes);
                }
            }
        }
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        self.save(if std::thread::panicking() {
            "failure"
        } else {
            "final"
        });
        let _ = self.call("DELETE", "", json!({}));
    }
}
