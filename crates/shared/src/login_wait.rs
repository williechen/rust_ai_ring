#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitState {
    pub failures: String,
    pub delay: String,
    pub until: String,
}

impl Default for WaitState {
    fn default() -> Self {
        Self {
            failures: "0".into(),
            delay: "0".into(),
            until: "0".into(),
        }
    }
}

fn natural(value: &str) -> Result<String, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("登入等待資料格式錯誤".into());
    }
    Ok(value.trim_start_matches('0').to_owned().pipe_zero())
}

trait Zero {
    fn pipe_zero(self) -> String;
}

impl Zero for String {
    fn pipe_zero(self) -> String {
        if self.is_empty() { "0".into() } else { self }
    }
}

fn less(a: &str, b: &str) -> bool {
    (a.len(), a) < (b.len(), b)
}

fn add(a: &str, b: &str) -> String {
    let (mut a, mut b) = (a.bytes().rev(), b.bytes().rev());
    let mut digits = Vec::new();
    let mut carry = 0u8;
    loop {
        let (x, y) = (a.next(), b.next());
        if x.is_none() && y.is_none() && carry == 0 {
            break;
        }
        let n = x.map_or(0, |v| v - b'0') + y.map_or(0, |v| v - b'0') + carry;
        digits.push(b'0' + n % 10);
        carry = n / 10;
    }
    digits.reverse();
    String::from_utf8(digits).expect("僅十進位數字")
}

fn subtract(a: &str, b: &str) -> String {
    let mut b = b.bytes().rev();
    let mut borrow = 0i16;
    let mut digits = Vec::new();
    for x in a.bytes().rev() {
        let mut n = i16::from(x - b'0') - i16::from(b.next().map_or(0, |v| v - b'0')) - borrow;
        borrow = i16::from(n < 0);
        if n < 0 {
            n += 10;
        }
        digits.push(b'0' + n as u8);
    }
    digits.reverse();
    String::from_utf8(digits)
        .expect("僅十進位數字")
        .trim_start_matches('0')
        .to_owned()
        .pipe_zero()
}

/// 十進位字串運算避免等待時間被機器整數範圍截斷；不設業務上限。
pub fn remaining(state: &WaitState, now: u64) -> Result<String, String> {
    let until = natural(&state.until)?;
    let now = now.to_string();
    Ok(if less(&now, &until) {
        subtract(&until, &now)
    } else {
        "0".into()
    })
}

/// 等待期間不增加失敗次數，也不重設等待起點。
pub fn failed(state: &WaitState, now: u64) -> Result<WaitState, String> {
    let failures = natural(&state.failures)?;
    let delay = natural(&state.delay)?;
    if remaining(state, now)? != "0" {
        return Ok(state.clone());
    }
    let delay = if delay == "0" {
        "15".into()
    } else {
        add(&delay, &delay)
    };
    let until = add(&now.to_string(), &delay);
    Ok(WaitState {
        failures: add(&failures, "1"),
        delay,
        until,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doubling_wait_and_no_restart() {
        let one = failed(&WaitState::default(), 100).unwrap();
        assert_eq!(one.delay, "15");
        assert_eq!(remaining(&one, 103).unwrap(), "12");
        assert_eq!(failed(&one, 104).unwrap(), one);
        let two = failed(&one, 115).unwrap();
        assert_eq!(two.delay, "30");
        let three = failed(&two, 145).unwrap();
        assert_eq!(three.delay, "60");
        assert_eq!(failed(&three, 205).unwrap().delay, "120");
    }
    #[test]
    fn durations_exceed_u64_without_saturation() {
        let s = WaitState {
            failures: "99".into(),
            delay: "18446744073709551616".into(),
            until: "0".into(),
        };
        let n = failed(&s, 100).unwrap();
        assert_eq!(n.delay, "36893488147419103232");
        assert_eq!(n.until, "36893488147419103332");
        assert_eq!(remaining(&n, 101).unwrap(), "36893488147419103231");
        assert_eq!(n.failures, "100");
    }
    #[test]
    fn malformed_persisted_data_fails_closed() {
        assert!(
            failed(
                &WaitState {
                    delay: "bad".into(),
                    ..Default::default()
                },
                0
            )
            .is_err()
        );
        assert!(
            remaining(
                &WaitState {
                    until: "-1".into(),
                    ..Default::default()
                },
                0
            )
            .is_err()
        );
    }
}
