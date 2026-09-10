# 合成 fixture

**AtCoder の実 HTML をここに置かないこと**（設計 §5.3）。問題文の著作権は AtCoder と作問者にある。

パーサのテストに必要なのは HTML の構造であって問題文ではないので、構造だけを写した
ダミー問題を置いている。実物は手元の `kyopro/acrust-fixtures/` にあり、
`tests/acceptance_abc418.rs`（`#[ignore]`）がそちらを見る。
