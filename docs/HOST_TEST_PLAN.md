# ShopRemote2 Host 버전 테스트 계획서

## 1. 개요

### 목적
ShopRemote2 Host(호스트 전용 버전)의 품질 보증 및 릴리스 준비. 호스트는 **수신 전용** 원격 접속을 받아들이며, 컨트롤러 기능은 모두 제거됨.

### 범위
- 기능 테스트 (Host 핵심 기능)
- 음성 테스트 (Controller 기능 제거 확인)
- 보안 테스트 (인증, 권한)
- 플랫폼 테스트 (Windows, macOS, Linux)
- 성능 테스트 (메모리, CPU, 바이너리 크기)

### 테스트 환경
| 항목 | 사양 |
|------|------|
| 개발 환경 | Rust 1.75+, Flutter |
| 빌드 도구 | cargo, Python 3.8+, vcpkg |
| CI/CD | GitHub Actions (Ubuntu 24.04) |
| 테스트 프레임워크 | cargo test, Flutter test |

---

## 2. 기능 테스트 (Functional Tests)

### 2.1 Host 서비스 초기화 및 시작

#### TC-FUNC-001: Host 서비스 부팅 시 자동 시작

**설명:** Host 서비스가 시스템 부팅 시 자동으로 시작되는지 검증

**사전 조건:**
- 호스트 애플리케이션 설치 완료
- 시스템 서비스 설정 완료

**테스트 단계:**
1. 호스트 머신 재부팅
2. 부팅 완료 후 약 5초 대기
3. 호스트 서비스 프로세스 실행 확인

**예상 결과:**
- ✅ 서비스 자동 시작 완료
- ✅ 원격 연결 대기 상태

**플랫폼별 검증:**
- **Windows**: `services.msc`에서 ShopRemote2 Host 서비스 상태 확인
- **macOS**: `launchctl list | grep shopremote2`로 LaunchAgent 확인
- **Linux**: `systemctl status shopremote2-host` 또는 `service shopremote2-host status` 확인

**Pass/Fail 기준:**
- 부팅 후 60초 이내 서비스 시작
- 서비스 상태: Running/Active
- CPU 사용률 < 5% (유휴 상태)

---

#### TC-FUNC-002: Host ID 및 비밀번호 생성

**설명:** 호스트 시작 시 고유 ID와 비밀번호가 자동 생성되는지 검증

**사전 조건:**
- 신규 설치 또는 설정 초기화

**테스트 단계:**
1. 호스트 애플리케이션 시작
2. 설정 파일 또는 UI를 통해 생성된 ID 확인
3. 생성된 비밀번호 확인
4. ID 형식 검증 (예: 숫자, 10-12자리)
5. 비밀번호 형식 검증 (예: 숫자, 6자리)

**예상 결과:**
- ✅ Host ID 자동 생성 및 저장
- ✅ 비밀번호 자동 생성 및 저장
- ✅ ID와 비밀번호 UI 또는 로그에 표시

**Pass/Fail 기준:**
- ID 생성 성공 여부
- 비밀번호 생성 성공 여부
- ID 형식 올바름 (숫자, 길이 10-12)
- 비밀번호 형식 올바름 (숫자, 길이 6)
- 생성 후 설정 파일에 암호화되어 저장됨

---

#### TC-FUNC-003: 비밀번호 갱신 (Refresh)

**설명:** Host 비밀번호 갱신 기능이 정상 작동하는지 검증

**사전 조건:**
- Host 서비스 실행 중
- 초기 비밀번호 기록

**테스트 단계:**
1. Host UI 또는 설정에서 "비밀번호 갱신" 버튼 클릭
2. 새로운 비밀번호 생성 확인
3. 이전 비밀번호와 다른지 검증
4. 새 비밀번호로 원격 연결 가능한지 확인

**예상 결과:**
- ✅ 새로운 비밀번호 생성
- ✅ 설정 파일에 저장
- ✅ 이전 비밀번호는 작동 불가
- ✅ 새 비밀번호로 연결 가능

**Pass/Fail 기준:**
- 비밀번호 갱신 버튼 응답 확인 (2초 이내)
- 새 비밀번호 생성 확인
- 이전 비밀번호 무효화
- 새 비밀번호로 인증 성공

---

### 2.2 원격 연결 기능

#### TC-FUNC-004: Full ShopRemote2 Client로부터 원격 연결

**설명:** Full ShopRemote2 Client에서 Host로 연결 가능한지 검증

**사전 조건:**
- Host 서비스 실행 중
- Full ShopRemote2 Client 설치 및 실행
- 동일 네트워크 또는 랑데뷰 서버 접근 가능

**테스트 단계:**
1. Client에서 Host ID 입력
2. Host 비밀번호 입력
3. 연결 버튼 클릭
4. 연결 상태 확인
5. 원격 화면 표시 확인

**예상 결과:**
- ✅ 연결 완료
- ✅ Host 화면 표시
- ✅ Client에서 Host 제어 가능

**Pass/Fail 기준:**
- 연결 시간: 10초 이내
- 화면 표시율: 100% (프레임 손실 없음)
- 연결 해제 후 재연결 가능

---

#### TC-FUNC-005: Host로의 파일 전송 (Client → Host)

**설명:** Client에서 Host로 파일 전송 기능 검증

**사전 조건:**
- Client와 Host 연결 상태
- 전송할 테스트 파일 준비 (1MB, 100MB, 1GB)

**테스트 단계:**
1. Client에서 파일 전송 초기화
2. 전송할 파일 선택
3. Host로 전송 시작
4. 진행률 확인
5. Host에서 파일 수신 확인
6. 파일 무결성 검증 (MD5/SHA256)

**예상 결과:**
- ✅ 파일 전송 완료
- ✅ 전송 속도 > 1MB/s (로컬)
- ✅ 전송된 파일 무결성 유지

**Pass/Fail 기준:**
- 작은 파일 (1MB) 전송 성공
- 중간 파일 (100MB) 전송 성공
- 큰 파일 (1GB) 전송 성공
- 파일 체크섬 일치
- 전송 중 오류 없음

---

#### TC-FUNC-006: 클립보드 공유

**설명:** Client와 Host 간 클립보드 공유 기능 검증

**사전 조건:**
- Client와 Host 연결 상태

**테스트 단계:**
1. Client에서 클립보드에 텍스트 복사
2. Host의 클립보드에 복사되는지 확인
3. Host의 클립보드에 파일 복사
4. Client의 클립보드에 전달되는지 확인
5. 매우 큰 텍스트 (10MB) 복사 테스트

**예상 결과:**
- ✅ 텍스트 클립보드 동기화 (< 1초)
- ✅ 파일 클립보드 동기화 (< 2초)
- ✅ 큰 데이터도 손실 없이 동기화

**Pass/Fail 기준:**
- 텍스트 복사 성공 (한글/영문/특수문자)
- 파일 복사 성공 (최대 1GB)
- 동기화 지연 < 1초
- 데이터 손실 없음

---

#### TC-FUNC-007: 화면 공유 (Screen Sharing)

**설명:** Host의 화면이 정상적으로 Client에 전송되는지 검증

**사전 조건:**
- Client와 Host 연결 상태
- Host에서 디스플레이 활동 발생 중

**테스트 단계:**
1. Host 화면의 변화 발생 (예: 창 열기, 이동)
2. Client 화면에 즉시 반영되는지 확인
3. 고속 애니메이션 재생 (비디오, 화면 스크롤)
4. Client에서 프레임 손실 확인
5. 특정 영역만 변경 시 해당 영역만 업데이트되는지 확인 (최적화)

**예상 결과:**
- ✅ 화면 업데이트 지연 < 100ms
- ✅ 프레임 레이트 >= 15fps
- ✅ 색상 정확도 > 95%

**Pass/Fail 기준:**
- 초기 화면 전송: 3초 이내
- 화면 업데이트 지연: < 100ms
- 프레임 레이트: >= 15fps (저사양), >= 30fps (고사양)
- 색상 왜곡 없음

---

#### TC-FUNC-008: 오디오 포워딩

**설명:** Host의 오디오가 Client로 정상 전송되는지 검증

**사전 조건:**
- Client와 Host 연결 상태
- Host에 스피커/헤드폰 연결
- 오디오 재생 가능한 환경

**테스트 단계:**
1. Host에서 오디오 재생 (예: YouTube, 음악 파일)
2. Client에서 오디오 수신 확인
3. 음질 확인 (codec, bitrate)
4. 지연 시간 측정
5. 오디오 끊김 확인

**예상 결과:**
- ✅ 오디오 재생 지연 < 500ms
- ✅ 음질 손실 최소화 (비트레이트 >= 128kbps)
- ✅ 오디오 끊김 없음

**Pass/Fail 기준:**
- 오디오 수신 성공
- 지연 < 500ms
- 비트레이트 >= 128kbps
- 오디오 잡음 없음

---

### 2.3 연결 로깅 및 설정 지속성

#### TC-FUNC-009: 연결 로깅

**설명:** 모든 원격 연결 시도가 로그에 기록되는지 검증

**사전 조건:**
- Host 서비스 실행 중
- 로깅 설정 활성화

**테스트 단계:**
1. Client에서 Host로 연결 시도
2. 성공한 연결 기록 확인
3. 실패한 연결 시도 기록 확인
4. 연결 해제 기록 확인
5. 로그 파일 위치 확인

**예상 결과:**
- ✅ 연결 시도: 기록됨
- ✅ 연결 성공: 시간, IP, Host ID, Client ID 포함
- ✅ 연결 실패: 실패 원인 포함
- ✅ 연결 해제: 세션 지속 시간 포함

**Pass/Fail 기준:**
- 모든 연결 이벤트 기록됨
- 타임스탬프 정확함
- 로그 파일 읽기 가능
- 로그 로테이션 정상 작동

---

#### TC-FUNC-010: 설정 지속성

**설명:** Host의 설정이 정상적으로 저장되고 복구되는지 검증

**사전 조건:**
- Host 서비스 실행 중

**테스트 단계:**
1. Host 설정 변경 (예: 비밀번호, 포트, 해상도 제한)
2. Host 서비스 재시작
3. 변경된 설정이 유지되는지 확인
4. 서비스 강제 종료 후 재시작
5. 설정 파일 손상 시뮬레이션 후 복구 확인

**예상 결과:**
- ✅ 설정 파일에 정상 저장
- ✅ 서비스 재시작 후 설정 유지
- ✅ 암호화 적용 (민감한 정보)

**Pass/Fail 기준:**
- 설정 변경 저장 성공
- 서비스 재시작 후 설정 유지
- 설정 암호화 확인
- 설정 파일 손상 시 기본값으로 복구

---

## 3. 음성 테스트 (Negative Tests)

### Controller 기능 제거 확인

#### TC-NEG-001: 원격 ID 입력 필드 없음

**설명:** Host에 원격 ID 입력 필드가 없는지 검증

**테스트 단계:**
1. Host UI 모든 탭/메뉴 검색
2. "연결 대상", "Remote ID", "접속" 등의 필드 찾기
3. 코드 검색: `remote_id_input`, `connection_input` 등

**예상 결과:**
- ❌ Host UI에 원격 ID 입력 필드 없음
- ❌ 나가는 연결 기능 없음

**Pass/Fail 기준:**
- UI에서 원격 ID 입력 필드 제거 확인
- 코드에서 outgoing connection 관련 코드 제거 확인

---

#### TC-NEG-002: 나가는 연결 기능 없음

**설명:** Host에서 다른 원격 데스크톱으로 연결할 수 없는지 검증

**테스트 단계:**
1. Host 코드 검색: `connect`, `client_mode`, `session.start`
2. Host 바이너리에 `client.rs` 기능 포함 여부 확인
3. Host 설정에 outgoing 연결 설정 없는지 확인

**예상 결과:**
- ❌ 나가는 연결 기능 없음
- ❌ Client 라이브러리 미포함

**Pass/Fail 기준:**
- Cargo feature에 client 기능 제거
- 컴파일 시 client 관련 코드 제외됨

---

#### TC-NEG-003: 원격 데스크톱 뷰어 없음

**설명:** Host에 원격 데스크톱 뷰어가 없는지 검증

**테스트 단계:**
1. Host UI에서 "뷰어", "Remote Desktop", "화면" 탭 찾기
2. 코드 검색: `viewer`, `remote_desktop`, `canvas`
3. Host 바이너리 크기 확인 (Full과 비교)

**예상 결과:**
- ❌ Host UI에 뷰어 없음
- ❌ Flutter 앱에서 뷰어 화면 제거됨

**Pass/Fail 기준:**
- Host의 `flutter/lib/` 코드에서 뷰어 관련 UI 제거
- 바이너리 크기 차이 확인 (Host < Full)

---

#### TC-NEG-004: 파일 전송 개시 불가

**설명:** Host에서 파일 전송을 개시할 수 없는지 검증

**테스트 단계:**
1. Host UI에서 "파일 전송", "File Manager" 탭 찾기
2. Host에서 파일을 Client로 보낼 방법 확인
3. 코드 검색: `send_files`, `initiate_transfer`

**예상 결과:**
- ❌ Host에서 파일 전송 개시 불가
- ❌ Host는 수신만 가능

**Pass/Fail 기준:**
- Host UI에서 파일 전송 개시 버튼 없음
- Host 코드에 `send_files` 함수 없음

---

#### TC-NEG-005: 주소록 / 저장된 연결 없음

**설명:** Host에 주소록이나 저장된 연결 목록이 없는지 검증

**테스트 단계:**
1. Host UI에서 "주소록", "연결 목록", "Connection Manager" 탭 찾기
2. Host 설정에 저장된 원격 주소 없는지 확인
3. 코드 검색: `address_book`, `peer_list`, `saved_connections`

**예상 결과:**
- ❌ Host에 주소록 없음
- ❌ Host는 수신 대기만 함

**Pass/Fail 기준:**
- Host UI에서 주소록 탭 제거됨
- Host 설정에 peer 저장 기능 없음

---

## 4. 보안 테스트 (Security Tests)

### TC-SEC-001: 비밀번호 보호

**설명:** 비밀번호 없이 Host로 연결할 수 없는지 검증

**사전 조건:**
- Host 서비스 실행 중
- Host 비밀번호 설정됨

**테스트 단계:**
1. Client에서 비밀번호 없이 연결 시도
2. 잘못된 비밀번호로 연결 시도 (3회)
3. 정확한 비밀번호로 연결 시도

**예상 결과:**
- ✅ 비밀번호 없는 연결: 거절
- ✅ 잘못된 비밀번호: 거절
- ✅ 정확한 비밀번호: 허용

**Pass/Fail 기준:**
- 비밀번호 없는 연결 거절됨
- 연결 거절 후 lockout 대기 시간 (권장: 5-10초)
- 정확한 비밀번호로 연결 성공

---

### TC-SEC-002: 로컬 권한 설정

**설명:** Host의 권한 설정이 정상 작동하는지 검증

**테스트 단계:**
1. Host 설정에서 "허용된 작업" 설정 (예: 파일 전송만 허용)
2. Client에서 제한된 작업 시도 (예: 키보드 입력)
3. 허용된 작업 수행 (예: 파일 전송)

**예상 결과:**
- ✅ 권한 설정 적용됨
- ✅ 권한 없는 작업: 거절
- ✅ 권한 있는 작업: 허용

**Pass/Fail 기준:**
- 권한 설정 저장됨
- 권한 적용 확인 (거절 메시지)
- 권한 있는 작업만 수행됨

---

### TC-SEC-003: 인증 실패 보호

**설명:** 반복된 인증 실패 시 Host가 일시적으로 차단하는지 검증

**사전 조건:**
- Host 서비스 실행 중

**테스트 단계:**
1. Client에서 잘못된 비밀번호로 5회 연결 시도
2. 6번째 연결 시도 (차단되어야 함)
3. 차단 해제 시간 대기 후 재시도

**예상 결과:**
- ✅ N회 실패 후 일시 차단 (권장: 5회 실패 후 10초 차단)
- ✅ 차단 기간 경과 후 재시도 가능

**Pass/Fail 기준:**
- 반복 실패 감지됨
- 일시 차단 적용됨
- 차단 해제 후 정상 작동

---

## 5. 플랫폼 테스트 (Platform Tests)

### 5.1 Windows 플랫폼

#### TC-PLAT-WIN-001: Windows 서비스 모드

**설명:** Host가 Windows 서비스로 정상 작동하는지 검증

**사전 조건:**
- Windows 10 이상
- 관리자 권한

**테스트 단계:**
1. Host 설치 (관리자 모드)
2. `services.msc` 열기
3. "ShopRemote2 Host" 서비스 확인
4. 서비스 상태: Running
5. 서비스 시작 유형: Automatic

**예상 결과:**
- ✅ 서비스 등록됨
- ✅ 서비스 실행 중
- ✅ 자동 시작 설정됨

**Pass/Fail 기준:**
- 서비스 설치 성공
- 서비스 상태: Running
- 시작 유형: Automatic

---

#### TC-PLAT-WIN-002: Windows 자동 시작

**설명:** Windows 부팅 시 Host 서비스 자동 시작 검증

**사전 조건:**
- Windows 서비스 설치됨

**테스트 단계:**
1. Windows 재부팅
2. 부팅 완료 후 5초 대기
3. `tasklist | find shopremote2` 명령으로 프로세스 확인
4. 네트워크 포트 listening 상태 확인

**예상 결과:**
- ✅ 부팅 후 자동 시작
- ✅ 프로세스 실행 중
- ✅ 포트 listening

**Pass/Fail 기준:**
- 부팅 후 60초 이내 서비스 시작
- 프로세스 존재함
- TCP/UDP 포트 listening 중

---

#### TC-PLAT-WIN-003: Windows 방화벽

**설명:** Host가 Windows 방화벽 규칙에 등록되는지 검증

**테스트 단계:**
1. 설정 > 개인 정보 및 보안 > Windows Defender 방화벽 > 앱 및 기능 열기
2. ShopRemote2 Host 규칙 확인
3. 규칙 활성화 상태 확인

**예상 결과:**
- ✅ 방화벽 규칙 등록됨
- ✅ 규칙: Private(개인) 네트워크 허용
- ✅ 규칙: Public(공용) 네트워크는 선택사항

**Pass/Fail 기준:**
- 방화벽 규칙 등록됨
- 규칙이 활성화됨
- 포트 접근 허용

---

### 5.2 macOS 플랫폼

#### TC-PLAT-MAC-001: macOS LaunchAgent 설치

**설명:** Host가 macOS LaunchAgent로 등록되는지 검증

**사전 조건:**
- macOS 10.14 이상
- Host 설치 완료

**테스트 단계:**
1. Terminal 열기
2. `launchctl list | grep shopremote2` 명령 실행
3. `~/.launchagents/com.shopremote2.host.plist` 파일 확인
4. 파일 내용에서 `<true/>` 설정 확인

**예상 결과:**
- ✅ LaunchAgent 등록됨
- ✅ 파일 위치: ~/.launchagents/
- ✅ 자동 시작 설정: enabled

**Pass/Fail 기준:**
- plist 파일 존재
- launchctl에 등록됨
- 부팅 시 자동 시작

---

#### TC-PLAT-MAC-002: macOS 권한 설정

**설명:** Host가 필요한 macOS 권한을 얻는지 검증

**테스트 단계:**
1. 시스템 설정 > 개인정보 및 보안 > 화면 기록 확인
2. ShopRemote2 Host 권한 확인
3. 접근성(Accessibility) 권한 확인
4. 키 모니터링 권한 확인

**예상 결과:**
- ✅ 화면 기록 권한 부여됨
- ✅ 접근성 권한 부여됨
- ✅ 키 모니터링 권한 부여됨

**Pass/Fail 기준:**
- 모든 필수 권한 부여됨
- 권한 없이 기능 실행 불가
- 권한 요청 프롬프트 표시

---

### 5.3 Linux 플랫폼

#### TC-PLAT-LINUX-001: systemd 서비스 설치

**설명:** Host가 systemd 서비스로 등록되는지 검증

**사전 조건:**
- Linux (Ubuntu, Fedora, CentOS 등)
- systemd 지원

**테스트 단계:**
1. Terminal 열기
2. `systemctl status shopremote2-host` 명령 실행
3. `/etc/systemd/system/shopremote2-host.service` 파일 확인
4. 서비스 활성화 상태 확인

**예상 결과:**
- ✅ 서비스 파일 등록됨
- ✅ 서비스 상태: Active
- ✅ 자동 시작 설정: enabled

**Pass/Fail 기준:**
- service 파일 존재: `/etc/systemd/system/shopremote2-host.service`
- `systemctl is-enabled shopremote2-host` 결과: enabled
- `systemctl is-active shopremote2-host` 결과: active

---

#### TC-PLAT-LINUX-002: Linux 자동 시작

**설명:** Linux 부팅 시 Host 서비스 자동 시작 검증

**사전 조건:**
- systemd 서비스 설치됨

**테스트 단계:**
1. 머신 재부팅
2. 부팅 완료 후 5초 대기
3. `systemctl status shopremote2-host` 명령 실행
4. 프로세스 확인: `ps aux | grep shopremote2`

**예상 결과:**
- ✅ 부팅 후 자동 시작
- ✅ 서비스 상태: Active
- ✅ 프로세스 실행 중

**Pass/Fail 기준:**
- 부팅 후 60초 이내 서비스 시작
- 서비스 상태: active (running)
- 프로세스 존재함

---

#### TC-PLAT-LINUX-003: Linux 방화벽 규칙

**설명:** Host가 Linux 방화벽에 규칙이 추가되는지 검증 (UFW/firewalld)

**테스트 단계:**
1. UFW 사용 시: `sudo ufw allow 5900` (예시 포트)
2. firewalld 사용 시: `sudo firewall-cmd --add-service=shopremote2-host --permanent`
3. 규칙 확인: `sudo ufw status` 또는 `sudo firewall-cmd --list-services`

**예상 결과:**
- ✅ 방화벽 규칙 추가됨
- ✅ Host 포트 허용됨

**Pass/Fail 기준:**
- 방화벽 규칙 확인됨
- Host 포트 접근 가능

---

## 6. 성능 테스트 (Performance Tests)

### TC-PERF-001: 메모리 사용량

**설명:** Host의 메모리 사용량이 Full 버전보다 낮은지 검증

**사전 조건:**
- Host와 Full 버전 모두 실행 중
- 동일한 조건 (OS, 네트워크 등)

**테스트 단계:**
1. 각 버전 실행 후 30초 대기
2. 메모리 사용량 측정 (RSS, VSZ)
3. 유휴 상태에서 10분 유지 후 메모리 측정
4. 원격 연결 상태에서 메모리 측정

**예상 결과:**
- ✅ Host 메모리 < Full 버전의 70%
- ✅ 유휴 상태: < 100MB (권장)
- ✅ 연결 중: < 150MB (권장)

**Pass/Fail 기준:**
| 상태 | Host | Full | 기준 |
|------|------|------|------|
| 유휴 | < 100MB | < 150MB | Host < Full × 70% |
| 연결 중 | < 150MB | < 250MB | Host < Full × 70% |
| 메모리 누수 | < 5MB/hr | < 5MB/hr | 유지 |

---

### TC-PERF-002: CPU 사용률

**설명:** Host의 CPU 사용률이 최소인지 검증

**사전 조건:**
- Host 서비스 실행 중
- 네트워크 활동 없음

**테스트 단계:**
1. Host 실행 후 30초 대기
2. 유휴 상태 CPU 사용률 측정 (5분)
3. 원격 연결 중 CPU 사용률 측정
4. 파일 전송 중 CPU 사용률 측정

**예상 결과:**
- ✅ 유휴 상태: < 1% CPU
- ✅ 연결 중 (비디오 스트림): < 20% CPU
- ✅ 파일 전송: < 15% CPU

**Pass/Fail 기준:**
| 상태 | CPU 사용률 |
|------|-----------|
| 유휴 | < 1% |
| 연결 중 (비디오) | < 20% |
| 파일 전송 | < 15% |
| 클립보드 공유 | < 2% |

---

### TC-PERF-003: 바이너리 크기

**설명:** Host 바이너리가 Full 버전보다 작은지 검증

**사전 조건:**
- 릴리스 빌드 완료

**테스트 단계:**
1. Host 바이너리 크기 측정
2. Full 버전 바이너리 크기 측정
3. 각 OS별 비교
4. 불필요한 기능 제거 확인

**예상 결과:**
- ✅ Host 크기 < Full 버전의 70%
- ✅ Controller 기능 제거로 인한 크기 감소

**Pass/Fail 기준:**
| OS | Host | Full | 기준 |
|----|------|------|------|
| Windows | < 50MB | < 80MB | Host < Full × 70% |
| macOS | < 60MB | < 100MB | Host < Full × 70% |
| Linux | < 40MB | < 70MB | Host < Full × 70% |

---

## 7. 통합 테스트 (Integration Tests)

### TC-INT-001: 다중 클라이언트 연결

**설명:** Host에 여러 Client가 동시에 연결할 수 있는지 검증

**사전 조건:**
- Host 서비스 실행 중
- 최소 3개의 Client 준비

**테스트 단계:**
1. Client 1 연결
2. Client 2 연결
3. Client 3 연결
4. 각 Client가 Host 제어 확인
5. Client 1 연결 해제
6. Client 2, 3 계속 연결 상태 확인

**예상 결과:**
- ✅ 최소 3개 Client 동시 연결 가능
- ✅ 한 Client의 연결 해제가 다른 Client에 영향 없음

**Pass/Fail 기준:**
- 동시 연결 수: >= 3
- 연결 간 간섭 없음
- 각 Client 독립 제어 가능

---

### TC-INT-002: 장시간 안정성

**설명:** Host가 장시간 안정적으로 작동하는지 검증

**사전 조건:**
- Host 서비스 실행 중

**테스트 단계:**
1. Host 시작 후 Client로 1분 연결
2. 연결 해제
3. 30분 대기
4. 다시 Client로 연결
5. 이 과정을 8시간 반복
6. 메모리 누수, 크래시 확인

**예상 결과:**
- ✅ 8시간 무중단 작동
- ✅ 메모리 누수 없음
- ✅ 크래시 없음

**Pass/Fail 기준:**
- 테스트 기간: 8시간
- 메모리 누수: < 10MB (8시간)
- 크래시: 0회

---

## 8. 승인 기준 (Acceptance Criteria)

### v1.0 릴리스 최소 요구사항

#### 필수 기능 (MUST HAVE)

| 항목 | 기준 | 테스트 케이스 |
|------|------|-------------|
| Host ID 생성 | ID가 자동 생성되어야 함 | TC-FUNC-002 |
| Host 비밀번호 생성 | 비밀번호가 자동 생성되어야 함 | TC-FUNC-002 |
| 원격 연결 수신 | Client의 연결을 수락해야 함 | TC-FUNC-004 |
| 화면 공유 | Host의 화면이 Client에 표시되어야 함 | TC-FUNC-007 |
| 파일 전송 (수신) | Client에서 Host로 파일 전송 가능 | TC-FUNC-005 |
| 클립보드 공유 | Client와 Host 간 클립보드 동기화 | TC-FUNC-006 |
| 비밀번호 보호 | 비밀번호로 접근 제어 | TC-SEC-001 |
| 자동 시작 | 시스템 부팅 시 자동 시작 | TC-FUNC-001 |

#### 필수 보안 (MUST HAVE)

| 항목 | 기준 | 테스트 케이스 |
|------|------|-------------|
| 인증 | 비밀번호 필수 | TC-SEC-001 |
| 권한 제어 | 권한에 따른 기능 제한 | TC-SEC-002 |
| 로깅 | 모든 연결 시도 기록 | TC-FUNC-009 |
| 암호화 | 설정 파일 암호화 저장 | TC-FUNC-010 |

#### 필수 플랫폼 (MUST HAVE)

| OS | 기준 | 테스트 케이스 |
|----|------|-------------|
| Windows | 서비스 모드, 자동 시작 | TC-PLAT-WIN-001, TC-PLAT-WIN-002 |
| macOS | LaunchAgent, 권한 설정 | TC-PLAT-MAC-001, TC-PLAT-MAC-002 |
| Linux | systemd 서비스, 자동 시작 | TC-PLAT-LINUX-001, TC-PLAT-LINUX-002 |

#### 음성 테스트 (MUST NOT HAVE)

| 항목 | 기준 | 테스트 케이스 |
|------|------|-------------|
| Controller 기능 | Controller 기능 완전 제거 | TC-NEG-001 ~ TC-NEG-005 |
| 나가는 연결 | 원격 연결 불가능 | TC-NEG-002 |
| 원격 데스크톱 뷰어 | 뷰어 UI 미포함 | TC-NEG-003 |

#### 성능 (SHOULD HAVE)

| 항목 | 목표 | 테스트 케이스 |
|------|------|-------------|
| 메모리 | < 100MB (유휴) | TC-PERF-001 |
| CPU | < 1% (유휴) | TC-PERF-002 |
| 바이너리 | < Full × 70% | TC-PERF-003 |
| 연결 시간 | < 10초 | TC-FUNC-004 |

---

## 9. 테스트 실행 계획

### 테스트 환경 준비

```bash
# 환경 변수 설정
export RUST_LOG=info,shopremote2=debug
export VCPKG_ROOT=/path/to/vcpkg

# Host 빌드 (Host 전용 feature 포함 시)
cargo build --release --features host-only

# 테스트 실행
cargo test --all

# Flutter 테스트 (UI 테스트)
cd flutter && flutter test
```

### CI/CD 파이프라인

```yaml
# GitHub Actions 통합
name: Host Test Suite

on: [push, pull_request]

jobs:
  functional-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run Cargo Tests
        run: cargo test --all --no-fail-fast
      - name: Run Negative Tests
        run: cargo test --test=host_negative_tests

  platform-tests:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Platform-specific Tests
        run: ./scripts/test_host_platform.sh

  performance-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Memory Test
        run: ./scripts/test_memory.sh
      - name: Binary Size Test
        run: ./scripts/test_binary_size.sh
```

---

## 10. 테스트 도구 및 명령어

### Rust 테스트

```bash
# 모든 테스트 실행
cargo test --all

# 특정 테스트 실행
cargo test test_host_init

# 테스트 출력 자세히 보기
cargo test -- --nocapture

# 성능 테스트
cargo test --release -- --nocapture --test-threads=1
```

### Flutter 테스트

```bash
# Flutter 유닛 테스트
cd flutter && flutter test

# Flutter 통합 테스트 (Host에서 실행)
cd flutter && flutter test integration_test/
```

### 플랫폼 테스트

```bash
# Windows (PowerShell)
Get-Service ShopRemote2Host | Start-Service
Get-Service ShopRemote2Host | Stop-Service

# macOS
launchctl load ~/Library/LaunchAgents/com.shopremote2.host.plist
launchctl unload ~/Library/LaunchAgents/com.shopremote2.host.plist

# Linux
sudo systemctl start shopremote2-host
sudo systemctl enable shopremote2-host
sudo systemctl status shopremote2-host
```

### 성능 테스트

```bash
# 메모리 모니터링 (Linux)
while true; do ps aux | grep shopremote2 | grep -v grep | awk '{print $6}'; sleep 1; done

# CPU 모니터링 (macOS)
top -l 1 -s 0 -n 0 | grep shopremote2

# Windows
tasklist /v | find "shopremote2"
```

---

## 11. 버그 보고 템플릿

### 발견된 버그 기록

```
## 버그 ID
HOST-BUG-[001]

## 제목
[플랫폼] [기능] 버그 설명

## 심각도
- Critical (기능 불가)
- High (기능 제한)
- Medium (부분 영향)
- Low (경미함)

## 테스트 케이스
TC-FUNC-XXX

## 재현 방법
1. 단계 1
2. 단계 2
3. 단계 3

## 예상 결과
...

## 실제 결과
...

## 로그
[로그 파일 첨부]

## 스크린샷
[이미지 첨부]
```

---

## 12. 체크리스트

### 릴리스 전 최종 검사

- [ ] 모든 필수 기능 테스트 완료 (TC-FUNC-001~010)
- [ ] 모든 음성 테스트 완료 (TC-NEG-001~005)
- [ ] 모든 보안 테스트 완료 (TC-SEC-001~003)
- [ ] 모든 플랫폼 테스트 완료 (TC-PLAT-*)
- [ ] 성능 요구사항 충족 (TC-PERF-001~003)
- [ ] 통합 테스트 완료 (TC-INT-*)
- [ ] 문서 업데이트 완료
- [ ] 릴리스 노트 작성 완료
- [ ] 보안 검수 완료
- [ ] 성능 검수 완료
- [ ] QA 승인 완료

---

## 13. 참고 자료

### 링크
- [ShopRemote2 GitHub](https://github.com/ccaplee/shopremote2)
- [ShopRemote2 보안 정책](./SECURITY.md)
- [Contributing Guide](./CONTRIBUTING.md)

### 관련 테스트 파일
- `src/server/` - Host 서버 기능
- `flutter/test/` - Flutter UI 테스트
- `.github/workflows/ci.yml` - CI/CD 파이프라인

### 버전 히스토리
| 버전 | 날짜 | 내용 |
|------|------|------|
| v1.0 | 2026-04-02 | 초기 테스트 계획 작성 |

---

## 부록: 테스트 데이터

### 테스트용 파일
- 작은 파일: `test_1mb.bin` (1MB)
- 중간 파일: `test_100mb.bin` (100MB)
- 큰 파일: `test_1gb.bin` (1GB)

### 테스트용 자격증명
```
Host ID: 123456789012
Password: 123456
```

### 테스트 네트워크 구성
```
+--------+              +--------+
| Client | <----------> | Host   |
+--------+              +--------+
(Local/Remote)      (Local Network)
```

---

**문서 버전**: v1.0
**작성일**: 2026-04-02
**최종 검토일**: [검토 예정]
**승인일**: [승인 예정]
