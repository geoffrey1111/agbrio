<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">

# Agbrio

**Agent Bridge — 判断とレビューを担う会話と、実装を担う会話をつなぐ。**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

計画用の会話と実行用の会話を分けていませんか？Agbrio は、その使い方のための引き継ぎツールです。結果を読み、次の作業に必要な部分を選び、受信側を確認して送信します。計画の文脈と実装の文脈を保ちながら、複数のプロジェクトでも正確な相手へ渡せます。

[Windows v0.1.25 をダウンロード](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.25) · [セットアップ](docs/SETUP.md) · [MIT](LICENSE)

## レビューから次の作業へ

![Agbrio の管理側と実行側。英語 UI と架空の会話](docs/assets/en/desktop-bridge.png)

管理側は計画・証拠の評価・次の判断を担当し、実行側はプロジェクトを変更して報告します。Bridge はタイトルではなく正確な会話 ID を結びます。

<p><img src="docs/assets/en/mobile-handoff.png" width="320" alt="指示だけを選択"> <img src="docs/assets/en/mobile-preview.png" width="320" alt="受信側と最終メッセージを確認"></p>

分析は元の会話に残し、指示だけを引き継げます。全文、選択した段落、ファイル、最終メッセージと送信先を確認してください。停滞後の短い更新だけでなく、前の詳細な結果もレビューできます。通知・Bridge 外の会話の監視・元の会話への返信・対応する Codex の質問への回答・署名付き更新も利用できます。

画面は実際の Agbrio コンポーネントをブラウザーで描画し、架空のデータを使用しています。日本語は紹介文の翻訳です。**アプリ UI は英語・簡体字中国語・繁体字中国語に対応**し、このページの画像は英語です。実機 iPhone の録画ではありません。

## AI アシスタントと連携

![クラウド接続の手順。英語 UI](docs/assets/en/assistant-guide.png)

Settings → AI assistant の6段階ガイドで ChatGPT / dot クラウドと Codex ローカルを選択します。自身の HTTPS MCP URL と有効な権限を使い、本人が OAuth 同意を行い、対象アシスタントでツールを検証します。ローカルの ready 表示だけではクラウド接続の成功になりません。手動ガイドとコピー用の指示が用意されています。

INSTANCE + CONVERSATION_REVIEW は30日間・取り消し可能です。アシスタントは最初に、担当する Bridge をユーザーに確認します。作業・方向・判断が必要な場合・停止条件を合意してから、通常の引き継ぎをレビューして進めます。未解決の判断は本人に質問し、実際の回答を記録します。権限を得ただけで全 Bridge が委任されたことにはなりません。

0.1.25 は公式 MCP Events に対応しています。イベント起動後、所有者が許可した範囲スキャンによる隔離テストの一度の引き渡しと受信結果を確認しました。イベント結果の自動注入は未解決で、無人の完全な E2E 検証ではありません。[Events guide](docs/MCP_EVENTS_WORKFLOW.md) · [Goal recovery limits](docs/GOAL_RECOVERY.md).

## 接続と動作条件

既存の Cloudflare Tunnel、Tailscale Funnel / Serve、HTTPS 入口を利用できます。運営者の引き換えコードを使うホスト接続は任意で、自己ホストも可能です。インスタンスの資格情報とルーティングは分離され、共通 MCP 資格情報を全ユーザーに渡す方式ではありません。コンピューターを起動・接続したままにし、スマートフォン PWA をペアリングします。

現在は Windows x64 + Codex + PWA の Alpha です。macOS と他の実行 Agent は未検証です。受信確認は、その後の作業完了を証明しません。選択ファイルは受信側プロジェクトへコピーされ、パスと SHA-256 が付きます。[ビルド](docs/BUILD.md) · [助手プロトコル](docs/ASSISTANT_MCP.md)。

[作者に連絡](mailto:geoffreyzjx@qq.com) · [問題を報告](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [改善を提案](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

## 使用量と接続ガイド

Codex の実際の使用量ウィンドウ、残りとリセット時刻を確認でき、手動で更新できます。AI アシスタント接続はクラウド／ローカルを選び、設定用の指示をコピー。ユーザーは認証と委任する Bridge の選択を行います。PWA は設定から手動更新でき、再インストールや再ペアリングは不要です。
