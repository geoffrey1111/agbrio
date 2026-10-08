<img src="docs/assets/agbrio-icon.png" width="72" alt="Agbrio">


# Agbrio

> **Windows プレビュー0.1.3：** [ダウンロード](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.3)。Bridgeの動作状況、停止後の確認通知、過去の回答選択、Codexの質問への回答とフォローアップ、設定の更新確認を追加しました。任意のホスト接続と無料セルフホストは引き続き利用できます。[変更と検証範囲](docs/RELEASE_0.1.3.md)。



**Agent Bridge — 管理する会話と実行する会話の引き継ぎをつなぐ。**

[English](README.md) · [简体中文](README.zh-CN.md) · [繁體中文](README.zh-TW.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

一つの Agent 会話でプロジェクトを管理し、別の会話で実際の作業をしていますか？ Agbrio はその使い方を中心に設計されています。

管理側の会話は計画を保ち、結果をレビューして次の仕事を決めます。実行側の会話はプロジェクトで変更を実装し、結果と検証資料を返します。**Bridge は、選んだ指示、結果、資料をこの二つの既存の会話の間で受け渡し、その都度ユーザーが送信を確認します。**

スマートフォンは、その引き継ぎを随時レビューして確認する入口です。製品の中心は会話間の橋渡しであり、リモートアクセスはワークベンチへ到達する手段です。

## 公開状況

**[v0.1.3](https://github.com/geoffrey1111/agbrio/releases/tag/v0.1.3) — MIT ライセンスの Windows Alpha。** インストーラーとチェックサムをダウンロードするか、ソースからビルドできます。クリーンな Windows のインストールとネイティブ初期設定の自動検証、実際の Codex／ファイル引き継ぎを別々に確認しています。アプリは英語・簡体字中国語・繁体字中国語に対応し、設定で切り替えられます。Mac と他の Agent は未対応です。[設定手順](docs/SETUP.md)。

## すでに二つの会話で協働している人向け

管理と実行の文脈を意識的に分ける人のためのワークフローツールです。二つの会話と役割はユーザーが選びます。Agbrio は Agent を自動配置せず、管理 Agent が自分の結果を自ら承認する仕組みも作りません。

**指示 → 実行側。結果と検証資料 → 管理側。最終確認 → ユーザー。**

## 一つの会話が管理し、もう一つが実行する。Bridge が引き継ぎをつなぐ。

![Managing and executing conversations side by side in one Bridge](docs/assets/desktop-bridge.png)

<p><img src="docs/assets/mobile-handoff.png" width="320" alt="Management-to-execution instruction selection"> <img src="docs/assets/mobile-return.png" width="320" alt="Execution-to-management result selection"></p>

PC の図は同じ Bridge の両側を並べた画面です。スマートフォンの図は管理側から実行側へ指示を渡す選択画面と、実行側から管理側へ結果を返す選択画面です。実際のコンポーネントに架空データを入れたブラウザープレビューで、私的会話や実機検証結果ではありません。スクリーンショットは中国語です。アプリは英語・簡体字中国語・繁体字中国語に対応し、日本語・韓国語は現在、紹介文書のみです。

## 個人実装でできること

- **Bridge** — 既存の会話を制御側と実行側に結び付けます。どちらの結果も読み、実行結果を制御側へ戻せます。
- **選択して引き継ぐ** — 全文を読み、ブロック単位で選択し、送信文を編集します。宛先を確認し、最後に一度だけ確認して送信します。指示の推薦は提案であり、選ぶのはユーザーです。
- **資料の引き継ぎ** — 選択した Codex のファイルは、宛先プロジェクトの `.aiwr/incoming/<handoff-id>/` にコピーされます。送信文に相対パスと SHA-256 が入り、受信側は実際のローカルコピーを読みます。ChatGPT の添付欄へのアップロードとは異なります。
- **通知と元の会話への返信** — 特定の結果通知を開き、その元の会話へ返信します。公開ユーザーメッセージと Agent の返信は古い順に同じタイムラインに並び、さらに古いページは上に読み込みます。
- **デスクトップ Host ＋ PWA** — PC が紐付け、観測結果、送信記録を管理し、スマートフォンはペアリングした Web セッションを使います。ライブの読み取りと送信には PC の起動、スリープ解除、ネット接続が必要です。

## 引き継ぎの流れ

1. Bridge を作り、制御側と実行側の正確な会話を指定します。
2. 制御側の返信を読み、指示ブロックと必要な資料を選びます。
3. 宛先を確認し、一度だけ確認して送信します。
4. 実行結果が戻ったら内容をレビューし、選んだ資料を制御側に返します。

## スマートフォンから PC への接続

![Agent Bridge topology](docs/assets/connection-flow.svg)

[インストールとペアリング](docs/SETUP.md) · [ソースからビルド](docs/BUILD.md) · [MIT](LICENSE)

デスクトップの設定 → デバイスで Cloudflare Tunnel、Tailscale Funnel、プライベートな Tailscale Serve、既存の HTTPS を選択できます。TLS とこの Host の識別情報を検証してから保存します。自分の Agent に渡す設定指示もコピーできます。Funnel/Serve は独自ドメインの購入が不要ですが、ご自身のサービスアカウントを使います。Tailscale の実アカウントでの設定は別途検証が必要です。Agbrio のホスト型中継サービスはありません。

## 確認、データ、送達

- 宛先は正確な conversation／thread ID で指定し、タイトルや画面位置から推測しません。
- 引き継ぎには明示的なユーザー確認が必要です。自律的な管理・実行ループは作りません。
- 送達が不明なら記録を確認し、無条件に再送しません。
- 選択ファイルの実体とハッシュが重要です。元の PC のパス表示だけでは、遠隔の受信側はファイルを取得できません。
- セルフホストでも、選んだネットワーク事業者と Agent 提供元への信頼は必要です。スマートフォンのキャッシュと保存メッセージも私的データとして扱います。

## 現時点の範囲

最初の対象は Windows x64＋Codex＋スマートフォン PWA です。アプリは英語・簡体字中国語・繁体字中国語に対応しています。日本語は現在、紹介文書のみです。Mac、他の Agent、ChatGPT ブラウザ連携は公開版の対応済み機能とはしていません。iPhone の修正報告と新しい Windows 環境の検証は区別します。

## ビルドと貢献

[設定手順](docs/SETUP.md)または[ビルド手順](docs/BUILD.md)から始めてください。正確な会話 ID、ユーザーの最終確認、不確実な送信の確認を維持します。プロジェクトは [MIT](LICENSE)、依存ソフトウェアは[それぞれのライセンス](THIRD_PARTY_NOTICES.md)に従います。

## 作者への連絡とフィードバック

[作者のプロフィール](mailto:geoffreyzjx@qq.com) · [GitHub](https://github.com/geoffrey1111) · [問題を報告する](https://github.com/geoffrey1111/agbrio/issues/new?template=bug_report.yml) · [ワークフローの改善を提案する](https://github.com/geoffrey1111/agbrio/issues/new?template=feature_request.yml)

使い方の質問も Issues に投稿できます。公開の場なので、デモデータを使い私的な情報を除いてください。
