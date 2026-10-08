<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">


# Agbrio

> **Windows 프리뷰0.1.3:** [다운로드](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.3). Bridge 실행 상태, 멈춘 뒤 검토 표시, 이전 결과 선택, Codex 질문 답변과 후속 제어, 설정의 업데이트 확인을 추가했습니다. 선택형 호스팅 코드와 무료 자체 배포도 유지됩니다. [변경 사항과 검증 범위](docs/RELEASE_0.1.3.md).



**Agent Bridge — 관리하는 대화와 실행하는 대화 사이의 인계.**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

한 Agent 대화에서 프로젝트를 관리하고 다른 대화에서 실제 작업을 수행하고 있나요? Agbrio는 이 사용 방식을 중심으로 설계되었습니다.

관리 대화는 계획을 유지하고 결과를 검토하며 다음 작업을 결정합니다. 실행 대화는 프로젝트에서 변경을 구현하고 결과와 검증 자료를 반환합니다. **Bridge는 선택한 지시, 결과, 자료를 두 기존 대화 사이에서 전달하며 매번 사용자가 전송을 확인합니다.**

휴대폰은 이 인계를 언제든 검토하고 확인하는 입구입니다. 제품의 중심은 대화 사이의 연결이며 원격 접속은 작업대에 접근하는 방법입니다.

## 공개 상태

**[v0.1.3](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.3) — MIT 라이선스 Windows Alpha.** 설치 프로그램과 체크섬을 다운로드하거나 소스로 빌드할 수 있습니다. 깨끗한 Windows 설치와 네이티브 초기 설정의 자동 검증, 실제 원격 Codex／파일 인계를 각각 확인했습니다. 앱 UI는 영어, 간체 중국어, 번체 중국어를 지원하며 설정에서 전환할 수 있습니다. Mac과 다른 Agent는 아직 지원 대상이 아닙니다. [설정 절차](docs/SETUP.md).

## 이미 두 대화로 협업하는 사용자를 위한 도구

관리와 실행의 맥락을 의도적으로 분리하는 사람을 위한 워크플로 도구입니다. 두 대화와 역할은 사용자가 선택합니다. Agbrio는 Agent를 자동 배정하거나 관리 Agent가 자기 결과를 스스로 승인하도록 만들지 않습니다.

**지시 → 실행 측. 결과와 검증 자료 → 관리 측. 최종 확인 → 사용자.**

## 한 대화는 관리하고, 다른 대화는 실행합니다. Bridge가 인계를 연결합니다.

![Managing and executing conversations side by side in one Bridge](docs/assets/desktop-bridge.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Management-to-execution instruction selection"> <img src="docs/assets/mobile-return.png" width="320" alt="Execution-to-management result selection"></p>

데스크톱 그림은 같은 Bridge의 양쪽 대화를 비교하는 화면입니다. 휴대폰 그림은 관리 측에서 실행 측으로 지시를 보내는 선택 화면과 실행 측에서 관리 측으로 결과를 반환하는 선택 화면입니다. 실제 컴포넌트에 가상 데이터를 넣은 브라우저 미리보기이며 개인 대화나 실기기 검증 결과가 아닙니다. 스크린샷은 중국어 UI입니다. 앱은 영어, 간체 중국어, 번체 중국어를 지원합니다. 한국어와 일본어는 현재 소개 문서에만 제공됩니다.

## 개인 구현에서 가능한 기능

- **Bridge** — 기존 대화를 제어 측과 실행 측으로 연결합니다. 양쪽 결과를 읽고 실행 결과를 제어 측으로 돌려보낼 수 있습니다.
- **선택하여 인계** — 전체 답변을 읽고 블록 단위로 선택하며 보낼 내용을 수정합니다. 수신 대상을 확인한 뒤 마지막에 한 번 확인하여 보냅니다. 지시 추천은 제안이며 선택은 사용자가 합니다.
- **관련 자료** — 선택한 Codex 인계 파일은 수신 프로젝트의 `.aiwr/incoming/<handoff-id>/`에 복사됩니다. 메시지에는 상대 경로와 SHA-256이 포함되고 수신 측은 실제 로컬 사본을 읽습니다. ChatGPT 첨부 슬롯에 파일을 업로드하는 방식은 아닙니다.
- **알림과 원래 대화에 답장** — 특정 결과 알림을 열고 정확한 원래 대화에 답장합니다. 공개 사용자 메시지와 Agent 답변은 오래된 순서로 하나의 타임라인에 표시되며 이전 페이지는 위쪽에 불러옵니다.
- **데스크톱 Host ＋ PWA** — 컴퓨터가 연결 정보, 관찰 결과, 전송 기록을 관리하고 휴대폰은 페어링한 웹 세션을 사용합니다. 실시간 읽기와 전송에는 컴퓨터 전원, 절전 해제, 인터넷 연결이 필요합니다.

## 인계 과정

1. Bridge를 만들고 제어 측과 실행 측의 정확한 대화를 연결합니다.
2. 제어 측 답변을 읽고 지시 블록과 필요한 자료를 선택합니다.
3. 수신 대상을 확인하고 한 번 확인하여 전송합니다.
4. 실행 결과가 돌아오면 검토하고 선택한 내용을 제어 측에 전달합니다.

## 휴대폰이 컴퓨터에 연결되는 방식

![Agent Bridge topology](docs/assets/connection-flow.svg)

[설치 및 페어링](docs/SETUP.md) · [소스 빌드](docs/BUILD.md) · [MIT](LICENSE)

데스크톱 설정 → 기기에서 Cloudflare Tunnel, Tailscale Funnel, 비공개 Tailscale Serve 또는 기존 HTTPS 주소를 선택합니다. 저장 전 TLS와 이 Host의 신원을 검증합니다. 자신의 Agent에 전달할 배포 지시도 복사할 수 있습니다. Funnel/Serve는 도메인 구매가 필요 없지만 자신의 서비스 계정을 사용합니다. Tailscale 실제 계정의 설정은 별도 검증이 필요합니다. Agbrio가 호스팅하는 중계 서비스는 없습니다.

## 확인, 데이터, 전달 상태

- 대상은 정확한 conversation／thread ID로 연결하며 제목이나 화면 위치로 추측하지 않습니다.
- 인계에는 명시적인 사용자 확인이 필요합니다. 자율적인 관리자／실행자 반복 루프를 만들지 않습니다.
- 전달 여부가 불확실하면 기록을 확인하고 무조건 다시 보내지 않습니다.
- 선택한 파일의 실제 바이트와 해시가 중요합니다. 원본 컴퓨터 경로만 표시해서는 원격 수신자가 파일을 받을 수 없습니다.
- 셀프 호스팅도 선택한 네트워크 제공자와 Agent 제공자를 신뢰해야 합니다. 휴대폰 캐시와 저장된 메시지도 개인 데이터입니다.

## 현재 범위

첫 지원 범위는 Windows x64＋Codex＋휴대폰 PWA입니다. 앱은 영어, 간체 중국어, 번체 중국어를 지원합니다. 한국어는 현재 소개 문서에만 제공됩니다. Mac, 다른 Agent, ChatGPT 브라우저 연동은 공개 버전의 지원 완료 기능으로 표시하지 않습니다. iPhone 수정 피드백과 새 Windows 환경 설치 검증은 구분합니다.

## 빌드 및 기여

[설정 문서](docs/SETUP.md) 또는 [빌드 문서](docs/BUILD.md)부터 시작하세요. 정확한 대화 ID, 사용자의 최종 확인, 불확실한 전달 상태 확인을 유지합니다. 프로젝트는 [MIT](LICENSE), 의존성은 [각자의 라이선스](THIRD_PARTY_NOTICES.md)를 따릅니다.

## 작성자 연락 및 피드백

[작성자 프로필](mailto:geoffreyzjx@qq.com) · [GitHub](https://github.com/geoffrey1111) · [문제 보고](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [워크플로 개선 제안](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

사용 관련 질문도 Issues에 남길 수 있습니다. 공개 채널이므로 예시 데이터를 사용하고 개인 정보를 제거해 주세요.
