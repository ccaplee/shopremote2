use hbb_common::{
    anyhow::anyhow,
    bail,
    config::Config,
    get_time,
    password_security::{decrypt_vec_or_original, encrypt_vec_or_original},
    ResultType,
};
use serde_derive::{Deserialize, Serialize};
use std::sync::Mutex;
use totp_rs::{Algorithm, Secret, TOTP};

/// 현재 활성화된 2FA 정보와 TOTP 객체를 저장합니다
lazy_static::lazy_static! {
    static ref CURRENT_2FA: Mutex<Option<(TOTPInfo, TOTP)>> = Mutex::new(None);
}

/// TOTP 발급자 이름 (QR 코드에 표시됨)
const ISSUER: &str = "ShopRemote2";
/// TOTP 태그 이름 (로그인 연결)
const TAG_LOGIN: &str = "Connection";

/// 2FA TOTP 정보 구조체
/// 비밀 키와 TOTP 생성 설정을 저장합니다
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TOTPInfo {
    /// TOTP를 생성할 계정/장치 이름
    pub name: String,
    /// TOTP 생성을 위한 비밀 키 (암호화되어 저장됨)
    pub secret: Vec<u8>,
    /// TOTP 코드의 자릿수 (보통 6)
    pub digits: usize,
    /// 2FA 생성 시간 (Unix 타임스탬프)
    pub created_at: i64,
}

impl TOTPInfo {
    /// TOTP 정보로부터 TOTP 객체를 생성합니다
    fn new_totp(&self) -> ResultType<TOTP> {
        let totp = TOTP::new(
            Algorithm::SHA1,
            self.digits,
            1,
            30,
            self.secret.clone(),
            Some(format!("{} {}", ISSUER, TAG_LOGIN)),
            self.name.clone(),
        )?;
        Ok(totp)
    }

    /// 새로운 TOTP 정보를 생성합니다 (임의의 비밀 키 생성)
    fn gen_totp_info(name: String, digits: usize) -> ResultType<TOTPInfo> {
        let secret = Secret::generate_secret();
        let totp = TOTPInfo {
            secret: secret.to_bytes()?,
            name,
            digits,
            created_at: get_time(),
            ..Default::default()
        };
        Ok(totp)
    }

    /// TOTP 정보를 JSON 문자열로 직렬화합니다 (비밀 키는 암호화됨)
    pub fn into_string(&self) -> ResultType<String> {
        let secret = encrypt_vec_or_original(self.secret.as_slice(), "00", 1024);
        let totp_info = TOTPInfo {
            secret,
            ..self.clone()
        };
        let s = serde_json::to_string(&totp_info)?;
        Ok(s)
    }

    /// JSON 문자열로부터 TOTP 객체를 복원합니다 (비밀 키는 자동으로 복호화됨)
    pub fn from_str(data: &str) -> ResultType<TOTP> {
        let mut totp_info = serde_json::from_str::<TOTPInfo>(data)?;
        let (secret, success, _) = decrypt_vec_or_original(&totp_info.secret, "00");
        if success {
            totp_info.secret = secret;
            return Ok(totp_info.new_totp()?);
        } else {
            bail!("decrypt_vec_or_original 2fa secret failed")
        }
    }
}

/// 새로운 2FA를 생성하고 QR 코드 URL을 반환합니다
/// 사용자는 인증 앱(Google Authenticator 등)에서 이 URL을 스캔합니다
pub fn generate2fa() -> String {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let id = crate::ipc::get_id();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let id = Config::get_id();
    if let Ok(info) = TOTPInfo::gen_totp_info(id, 6) {
        if let Ok(totp) = info.new_totp() {
            let code = totp.get_url();
            *CURRENT_2FA.lock().unwrap() = Some((info, totp));
            return code;
        }
    }
    "".to_owned()
}

/// 사용자가 제공한 2FA 코드를 검증하고, 검증 성공 시 저장합니다
/// 코드: 인증 앱에서 표시되는 6자리 코드
pub fn verify2fa(code: String) -> bool {
    if let Some((info, totp)) = CURRENT_2FA.lock().unwrap().as_ref() {
        if let Ok(res) = totp.check_current(&code) {
            if res {
                if let Ok(v) = info.into_string() {
                    // 검증된 2FA 정보를 설정에 저장
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    crate::ipc::set_option("2fa", &v);
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    Config::set_option("2fa".to_owned(), v);
                    return res;
                }
            }
        }
    }
    false
}

/// 저장된 2FA 정보를 로드합니다
/// 설정에서 저장된 TOTP 정보를 복원하거나 제공된 원본 데이터를 사용합니다
pub fn get_2fa(raw: Option<String>) -> Option<TOTP> {
    TOTPInfo::from_str(&raw.unwrap_or(Config::get_option("2fa")))
        .map(|x| Some(x))
        .unwrap_or_default()
}

/// Telegram 봇 설정 구조체
/// 2FA 코드를 Telegram으로 전송하기 위한 설정을 저장합니다
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramBot {
    /// 직렬화되지 않는 토큰 문자열 (실시간 사용)
    #[serde(skip)]
    pub token_str: String,
    /// 암호화된 토큰 바이트 배열 (저장용)
    pub token: Vec<u8>,
    /// Telegram 채팅 ID (메시지를 받을 대상)
    pub chat_id: String,
}

impl TelegramBot {
    /// Telegram 봇 설정을 JSON 문자열로 직렬화합니다 (토큰은 암호화됨)
    fn into_string(&self) -> ResultType<String> {
        let token = encrypt_vec_or_original(self.token_str.as_bytes(), "00", 1024);
        let bot = TelegramBot {
            token,
            ..self.clone()
        };
        let s = serde_json::to_string(&bot)?;
        Ok(s)
    }

    /// Telegram 봇 설정을 저장합니다
    fn save(&self) -> ResultType<()> {
        let s = self.into_string()?;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        crate::ipc::set_option("bot", &s);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        Config::set_option("bot".to_owned(), s);
        Ok(())
    }

    /// 저장된 Telegram 봇 설정을 로드합니다 (토큰 자동 복호화)
    pub fn get() -> ResultType<Option<TelegramBot>> {
        let data = Config::get_option("bot");
        if data.is_empty() {
            return Ok(None);
        }
        let mut bot = serde_json::from_str::<TelegramBot>(&data)?;
        let (token, success, _) = decrypt_vec_or_original(&bot.token, "00");
        if success {
            bot.token_str = String::from_utf8(token)?;
            return Ok(Some(bot));
        }
        bail!("decrypt_vec_or_original telegram bot token failed")
    }
}

/// Telegram 봇을 통해 2FA 코드를 전송합니다
/// 참고: https://gist.github.com/dideler/85de4d64f66c1966788c1b2304b9caf1
pub async fn send_2fa_code_to_telegram(text: &str, bot: TelegramBot) -> ResultType<()> {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot.token_str);
    let params = serde_json::json!({"chat_id": bot.chat_id, "text": text});
    crate::post_request(url, params.to_string(), "").await?;
    Ok(())
}

/// Telegram 봇의 채팅 ID를 가져옵니다
/// 봇에 보낸 메시지로부터 chat_id를 추출하고 설정에 저장합니다
pub fn get_chatid_telegram(bot_token: &str) -> ResultType<Option<String>> {
    let url = format!("https://api.telegram.org/bot{}/getUpdates", bot_token);
    // 호출자가 tokio 런타임에 있으므로 새 스레드에서 동기 요청을 호출해야 합니다
    let handle = std::thread::spawn(move || crate::post_request_sync(url, "".to_owned(), ""));
    let resp = handle.join().map_err(|_| anyhow!("Thread panicked"))??;
    let value = serde_json::from_str::<serde_json::Value>(&resp).map_err(|e| anyhow!(e))?;

    // 응답의 오류 코드 확인
    if let Some(error_code) = value.get("error_code").and_then(|code| code.as_i64()) {
        // 오류 코드가 있으면 설명을 에러 메시지로 사용합니다
        let description = value["description"]
            .as_str()
            .unwrap_or("Unknown error occurred");
        return Err(anyhow!(
            "Telegram API error: {} (error_code: {})",
            description,
            error_code
        ));
    }

    let chat_id = &value["result"][0]["message"]["chat"]["id"];
    let chat_id = if let Some(id) = chat_id.as_i64() {
        Some(id.to_string())
    } else if let Some(id) = chat_id.as_str() {
        Some(id.to_owned())
    } else {
        None
    };

    // 채팅 ID가 가져왔으면 봇 설정을 저장합니다
    if let Some(chat_id) = chat_id.as_ref() {
        let bot = TelegramBot {
            token_str: bot_token.to_owned(),
            chat_id: chat_id.to_owned(),
            ..Default::default()
        };
        bot.save()?;
    }

    Ok(chat_id)
}
