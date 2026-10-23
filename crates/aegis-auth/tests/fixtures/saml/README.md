# SAML の署名テスト用フィクスチャ

`signed_assertion.xml`（アサーションに RSA-SHA256 の署名、`InclusiveNamespaces PrefixList="xs"`）と
`signed_response.xml`（応答全体に RSA-SHA512 の署名）は、`aegis-auth` の正規化ではなく libxml2 の
Exclusive C14N（lxml）で署名したものです。応答は [signxml](https://github.com/XML-Security/signxml) で署名し、
アサーションは `PrefixList` を参照の変換（`Transform`）にも置くため（Okta・AD FS と同じ形。signxml は
`CanonicalizationMethod` にしか置かない）、`make_fixtures.py` で組み立てています。
自前の署名と検証で正規化の誤りが打ち消し合わないよう、別の実装で作った署名を検証できることを確かめます。

- 整形の空白・使われない名前空間宣言・既定の名前空間・属性の順序・エスケープ（`&amp;` `&#13;` `&#10;`）・
  NameID 内のコメントを含みます。
- `idp_cert.pem` は検証用の自己署名証明書です。秘密鍵は生成後に破棄しており、リポジトリにはありません。
- 日時や `InResponseTo` は固定値のため、`dsig` の署名検証のテストだけで使います（応答全体の検証は
  `saml::mock::MockSamlIdp` で行います）。

作り直すには `pip install signxml` の上で `python make_fixtures.py <このディレクトリ>` を実行します
（鍵と証明書は毎回作り直されます）。
