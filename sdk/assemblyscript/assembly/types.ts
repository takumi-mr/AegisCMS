/**
 * AegisCMS AssemblyScript Plugin SDK - Types
 */

import { JsonValue } from "./jsonvalue";

export class PostContext {
  id: string = "";
  title: string = "";
  slug: string = "";
  content_type: string = "article";
  status: string = "draft";
  author_id: string = "";
  blocks_json: string = "[]";
  created_at: i64 = 0;
  updated_at: i64 = 0;

  constructor(title: string = "", slug: string = "") {
    this.title = title;
    this.slug = slug;
  }
}

export class BlockNode {
  name: string = "core/paragraph";
  text: string | null = null;
  attributes: Map<string, string> = new Map<string, string>();
  children: Array<BlockNode> = new Array<BlockNode>();

  constructor(name: string = "core/paragraph", text: string | null = null) {
    this.name = name;
    this.text = text;
  }

  /**
   * ホストから渡されたブロック（`render-blocks` の入力など）を読む。
   * 属性は文字列で持つ（文字列以外の値は JSON の表記になる。`level: 2` は "2"、`true` は "true"）
   */
  static fromJson(v: JsonValue): BlockNode {
    const node = new BlockNode(v.get("name").asString(), v.get("text").asNullableString());
    const attributes = v.get("attributes");
    if (attributes.isObject()) {
      const keys = attributes.fields.keys();
      for (let i = 0; i < keys.length; ++i) {
        node.attributes.set(keys[i], attributes.fields.get(keys[i]).asText());
      }
    }
    const children = v.get("children");
    for (let i = 0; i < children.length; ++i) node.children.push(BlockNode.fromJson(children.at(i)));
    return node;
  }

  static heading(level: i32, text: string): BlockNode {
    const node = new BlockNode("core/heading", text);
    node.attributes.set("level", level.toString());
    return node;
  }

  static paragraph(text: string): BlockNode {
    return new BlockNode("core/paragraph", text);
  }
}

/**
 * `on_render_page` で扱うページ情報（ホストの `PageRenderContext` の文字列フィールド）。
 * ホストは返した JSON のフィールドだけを上書きするため、本文 (`content_html`) や記事一覧
 * (`items`) などここに無いフィールドはそのまま保たれる
 */
export class PageRenderContext {
  site_title: string = "";
  site_description: string = "";
  template: string = "";
  content_type: string = "";
  title: string = "";
  slug: string = "";
  /** 本文（枠を除く）のバイト数。`content_html` を受け取らないプラグインでも本文があるかを調べられる */
  content_length: i32 = 0;
  extra_css: string = "";
  extra_head: string = "";
  /** 本文の前に足す HTML（書き込み専用。ホストが全プラグインのあとで本文と結合する） */
  prepend_content: string = "";
  /** 本文の後ろに足す HTML（書き込み専用） */
  append_content: string = "";
  /**
   * `<head>` に足すスクリプト（書き込み専用。ホストが検査して `<script>` のタグにする）。
   * スクリプトは、ここでだけ足せる: `extra_head`・`prepend_content`・`append_content` の中の
   * `<script>` と `on*` 属性は、ホストが取り除く（docs/csp.md の 4）
   */
  scripts: InjectedScript[] = [];
  /**
   * 宣言的クエリの結果（クエリ名 → 結果の JSON。docs/queries.md）。
   * マニフェストの `capabilities.render_page.queries` に書いた名前の分だけ渡る（読み取り専用）
   */
  queries: JsonValue = JsonValue.object();

  /**
   * ホストが `<head>` に出す HTML（`extra_head` の後ろに `scripts` のタグ）。
   * プラグインのテストで、ホストを通したあとの `<head>` を確かめるために使う
   */
  headHtml(): string {
    let html = this.extra_head;
    for (let i = 0; i < this.scripts.length; ++i) {
      const script = this.scripts[i];
      if (html.length > 0 && !html.endsWith("\n")) {
        html += "\n";
      }
      const consent =
        script.consent.length > 0
          ? " type=\"text/plain\" data-aegis-consent=\"" + script.consent + "\""
          : "";
      if (script.kind == "external") {
        html +=
          "<script"
          + consent
          + " src=\""
          + script.url
          + "\""
          + (script.async_load ? " async" : "")
          + (script.defer ? " defer" : "")
          + (script.crossorigin ? " crossorigin=\"anonymous\"" : "")
          + "></script>";
      } else {
        html += "<script" + consent + ">" + script.code + "</script>";
      }
    }
    return html;
  }
}

/**
 * `<head>` に足すスクリプト 1 つ。ホストがマニフェストの宣言と照合して検査し、タグを作る
 * （属性はホストが決める）。
 * - 外部（`external`）: URL は、このサイトのパスか、`capabilities.csp.script_domains` にある `https://` の URL
 * - インライン（`inline`）: マニフェストに `capabilities.inline_scripts: true` が要る
 * `consent` に区分（`analytics` など）を入れると、ホストが同意のあとに実行する形で出す
 */
export class InjectedScript {
  kind: string = "inline";
  url: string = "";
  code: string = "";
  async_load: bool = false;
  defer: bool = false;
  crossorigin: bool = false;
  consent: string = "";

  static inline(code: string): InjectedScript {
    const script = new InjectedScript();
    script.kind = "inline";
    script.code = code;
    return script;
  }

  static external(url: string): InjectedScript {
    const script = new InjectedScript();
    script.kind = "external";
    script.url = url;
    return script;
  }

  /** 同意の区分を付ける（同意があるまで実行しない） */
  withConsent(category: string): InjectedScript {
    this.consent = category;
    return this;
  }

  /** `async` にする（外部のスクリプトだけ） */
  asyncLoad(): InjectedScript {
    if (this.kind == "external") this.async_load = true;
    return this;
  }

  /** `defer` にする（外部のスクリプトだけ） */
  deferred(): InjectedScript {
    if (this.kind == "external") this.defer = true;
    return this;
  }

  /** `crossorigin="anonymous"` を付ける（外部のスクリプトだけ。CORS を許すサーバーのものに限る） */
  withCrossorigin(): InjectedScript {
    if (this.kind == "external") this.crossorigin = true;
    return this;
  }
}
