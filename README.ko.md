# Agbrio

**Agent는 PC에서 계속 일하고, 진행 상황은 휴대폰에서 확인하세요.**

[![Download](https://img.shields.io/github/v/release/geoffrey1111/agbrio?label=download)](https://github.com/geoffrey1111/agbrio/releases/latest)
[![Build](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml/badge.svg?branch=main)](https://github.com/geoffrey1111/agbrio/actions/workflows/windows-alpha.yml)
[![MIT](https://img.shields.io/github/license/geoffrey1111/agbrio)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-x64-0078D4)](https://github.com/geoffrey1111/agbrio/releases/latest)

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

Codex와 Claude Code 대화를 확인하고, 메시지를 보내 후속 작업을 진행하세요. 새 결과나 결정이 필요할 때 알림을 받습니다. 계획과 실행을 나누거나 하나의 관리 대화에 여러 실행 대화를 맡길 수 있습니다.

![Desktop conversation overview](docs/assets/en/conversations-current.png)

## 자리를 비워도 진행 상황을 보고 답장하기

실행 중, 확인 대기, 하위 대화 상태를 한눈에 확인하세요. 원래 대화에서 결과를 읽고 추가 지시를 보내거나 지원되는 질문에 답할 수 있습니다.

<p align="center"><img src="docs/assets/en/mobile-conversations-current.png" width="300" alt="Phone conversation list and activity"> &nbsp; <img src="docs/assets/en/mobile-reply-current.png" width="300" alt="Reply to the original conversation from your phone"></p>

## 대화를 선택하고 지켜보거나 연결하기

PC의 기존 대화를 선택하세요. 하나를 모니터링하거나 관리자와 실행자를 Bridge로 연결할 수 있습니다. 전달할 내용과 대상을 확인한 뒤 직접 승인합니다.

<p align="center"><img src="docs/assets/en/select-conversation-current.png" width="300" alt="Choose a conversation to watch"> &nbsp; <img src="docs/assets/en/review-current.png" width="300" alt="Review content and recipient before sending"></p>

![Choose a planning conversation and executor](docs/assets/en/binding-current.png)

## 관리 대화에 일상적인 후속 작업 맡기기

하나의 관리 대화가 여러 Codex 대화의 진행 상황을 읽고, 작업을 전달하며, 결과를 정리합니다. 판단이 필요한 일은 사용자에게 돌아옵니다. Claude Code나 Codex를 관리 대화로 사용할 수 있습니다.

```mermaid
flowchart TD
  U["사용자 · 방향과 주요 결정"] <--> M["관리 대화 · Claude Code / Codex"]
  M <--> A["Codex · 화면 구현"]
  M <--> B["Codex · API 개발"]
  M <--> C["Codex · 검증"]
```

위임한 메시지는 Codex로 전송됩니다. Claude Code에는 휴대폰에서 직접 답장할 수 있습니다. 결정과 위임 범위는 사용자가 관리합니다.

## 시작하기

1. Windows 앱을 내려받고 사용 중인 Agent를 연결하세요.
2. 모니터링할 대화를 선택하거나 관리자와 실행자를 연결하세요.
3. 설정 → 기기에서 휴대폰을 페어링하고 Agbrio 주소를 여세요.

PC와 Agbrio는 온라인 상태여야 합니다. 알림을 받을 기기에서 알림을 켜세요.

[Download](https://github.com/geoffrey1111/agbrio/releases/latest) · [설정 안내](docs/START_HERE.md)

<details>
<summary>함께 쓰기 좋은 기능</summary>

대화에 연결된 문서, 이미지, 프로젝트 보드도 볼 수 있습니다. 맞춤 연동에는 API / MCP를 사용할 수 있습니다.

</details>

<sub>현재 인터페이스에 가상의 예시 데이터를 사용한 화면입니다.</sub>


## macOS 미리 보기 후보 (Apple Silicon)

macOS 지원은 개발 중이며 Windows와 기능이 같지 않습니다. 수동 macOS candidate DMG Actions는 macOS 13.5+ / Apple Silicon용 ad-hoc 서명 .dmg artifact만 생성하며 Release 게시나 업데이트 비밀 키 등록은 하지 않습니다. macOS source에서 검사 및 테스트합니다. 실제 성공 전 빌드는 미검증이며 일부 기능은 미지원으로 표시됩니다.

체크섬 확인 후 Agbrio.app을 Applications에 복사합니다. 신뢰한 미서명/ad-hoc 후보가 차단되면 시스템 설정 → 개인정보 보호 및 보안 → 확인 없이 열기를 사용하거나 소유자가 아래 명령을 실행합니다. 앱은 quarantine을 자동 해제하지 않습니다. Apple Developer 계정은 필요 없지만 Developer ID 서명과 공증은 없습니다.

```text
xattr -dr com.apple.quarantine /Applications/Agbrio.app
```

구현된 기능에 필요한 알림, 선택 파일의 파일 및 폴더/이동식 볼륨, 보호 위치의 전체 디스크 접근만 허용합니다. 향후 읽기 전용 AX 복원 시 손쉬운 사용 권한이 필요합니다. 기존 브라우저 경로는 비활성입니다. Pi 권한 상승, 전체 로컬 볼륨 권한, 특권 helper, 서명된 앱 내 업데이트는 미구현입니다. ad-hoc 서명은 빌드마다 바뀌므로 업데이트 후 권한을 다시 허용해야 할 수 있습니다. CI/패키징/시작 검사는 기능 동등성이나 사용자 UAT가 아닙니다.
