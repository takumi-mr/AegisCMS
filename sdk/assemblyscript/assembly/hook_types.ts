/**
 * AegisCMS AssemblyScript Plugin SDK - Hook payloads
 *
 * `calculate-order` / `on-order-status-changed` / `send-notification` / `on-content-published` /
 * `check-comment` / `render-blocks` の入出力。形は docs/plugin_hooks.md の 2 章に対応する
 */

import { JsonValue } from "./jsonvalue";

/** 注文の 1 行（商品 × 数量）。価格はホストが商品から決める（プラグインが変えても無視される） */
export class OrderLine {
  product_id: string = "";
  variant_id: string | null = null;
  variant_title: string | null = null;
  slug: string = "";
  title: string = "";
  sku: string | null = null;
  /** 単価（通貨の最小単位の整数。JPY なら円） */
  unit_price: i64 = 0;
  quantity: i64 = 0;
  line_total: i64 = 0;

  static fromJson(v: JsonValue): OrderLine {
    const line = new OrderLine();
    line.product_id = v.get("product_id").asString();
    line.variant_id = v.get("variant_id").asNullableString();
    line.variant_title = v.get("variant_title").asNullableString();
    line.slug = v.get("slug").asString();
    line.title = v.get("title").asString();
    line.sku = v.get("sku").asNullableString();
    line.unit_price = v.get("unit_price").asInt();
    line.quantity = v.get("quantity").asInt();
    line.line_total = v.get("line_total").asInt();
    return line;
  }
}

/** 送料・税・割引などの加算・減算（割引は負の金額） */
export class OrderAdjustment {
  /** `shipping` / `tax` / `discount` / `fee` など */
  kind: string = "";
  /** 表示名（カート・注文確認ページ・決済ページに出る） */
  label: string = "";
  amount: i64 = 0;
  code: string | null = null;
  /** 追加したプラグイン（ホストが設定する） */
  source: string = "";

  constructor(kind: string = "", label: string = "", amount: i64 = 0) {
    this.kind = kind;
    this.label = label;
    this.amount = amount;
  }

  static fromJson(v: JsonValue): OrderAdjustment {
    const adjustment = new OrderAdjustment(
      v.get("kind").asString(),
      v.get("label").asString(),
      v.get("amount").asInt(),
    );
    adjustment.code = v.get("code").asNullableString();
    adjustment.source = v.get("source").asString();
    return adjustment;
  }

  toJson(): JsonValue {
    return JsonValue.object()
      .setString("kind", this.kind)
      .setString("label", this.label)
      .setInt("amount", this.amount)
      .set("code", JsonValue.fromNullableString(this.code))
      .setString("source", this.source);
  }
}

/** 配送先 */
export class ShippingAddress {
  name: string = "";
  postal_code: string = "";
  region: string = "";
  city: string = "";
  line1: string = "";
  line2: string = "";
  phone: string = "";
  country: string = "";

  static fromJson(v: JsonValue): ShippingAddress {
    const address = new ShippingAddress();
    address.name = v.get("name").asString();
    address.postal_code = v.get("postal_code").asString();
    address.region = v.get("region").asString();
    address.city = v.get("city").asString();
    address.line1 = v.get("line1").asString();
    address.line2 = v.get("line2").asString();
    address.phone = v.get("phone").asString();
    address.country = v.get("country").asString();
    return address;
  }
}

/**
 * 見積もり・注文（`calculate-order` / `on-order-status-changed`）。
 * `calculate-order` でホストが反映するのは `adjustments` と `metadata` だけ
 */
export class OrderContext {
  order_id: string | null = null;
  order_number: string | null = null;
  /** ISO 4217 の通貨コード（小文字） */
  currency: string = "";
  lines: Array<OrderLine> = new Array<OrderLine>();
  adjustments: Array<OrderAdjustment> = new Array<OrderAdjustment>();
  /** 商品の合計 */
  subtotal: i64 = 0;
  /** 支払う金額（ホストが `subtotal` + `adjustments` で計算し直す） */
  total: i64 = 0;
  customer_email: string = "";
  customer_name: string = "";
  shipping_address: ShippingAddress | null = null;
  coupon_code: string | null = null;
  /** `pending_payment` / `paid` / `shipped` / `completed` / `cancelled`（見積もり中は null） */
  status: string | null = null;
  /** プラグインが付ける任意の値（注文に保存される。JSON のオブジェクト） */
  metadata: JsonValue = JsonValue.object();

  static fromJson(v: JsonValue): OrderContext {
    const order = new OrderContext();
    order.order_id = v.get("order_id").asNullableString();
    order.order_number = v.get("order_number").asNullableString();
    order.currency = v.get("currency").asString();
    const lines = v.get("lines");
    for (let i = 0; i < lines.length; ++i) order.lines.push(OrderLine.fromJson(lines.at(i)));
    const adjustments = v.get("adjustments");
    for (let i = 0; i < adjustments.length; ++i) {
      order.adjustments.push(OrderAdjustment.fromJson(adjustments.at(i)));
    }
    order.subtotal = v.get("subtotal").asInt();
    order.total = v.get("total").asInt();
    order.customer_email = v.get("customer_email").asString();
    order.customer_name = v.get("customer_name").asString();
    const address = v.get("shipping_address");
    if (address.isObject()) order.shipping_address = ShippingAddress.fromJson(address);
    order.coupon_code = v.get("coupon_code").asNullableString();
    order.status = v.get("status").asNullableString();
    const metadata = v.get("metadata");
    if (metadata.isObject()) order.metadata = metadata;
    return order;
  }

  /** 送料・税などを除いた、割引を含む商品の金額（`subtotal` + 割引） */
  discountedSubtotal(): i64 {
    let total = this.subtotal;
    for (let i = 0; i < this.adjustments.length; ++i) {
      if (this.adjustments[i].amount < 0) total += this.adjustments[i].amount;
    }
    return total;
  }

  /** `calculate-order` の出力。ホストが読むのは `adjustments` と `metadata` だけなので、その 2 つを返す */
  toResultJson(): JsonValue {
    const adjustments = JsonValue.array();
    for (let i = 0; i < this.adjustments.length; ++i) adjustments.push(this.adjustments[i].toJson());
    return JsonValue.object().set("adjustments", adjustments).set("metadata", this.metadata);
  }
}

/** 注文の作成・状態の変化（`on-order-status-changed`） */
export class OrderStatusChange {
  order: OrderContext = new OrderContext();
  /** 変化前の状態（注文の作成時は null） */
  from_status: string | null = null;
  to_status: string | null = null;
  occurred_at: string = "";

  static fromJson(v: JsonValue): OrderStatusChange {
    const change = new OrderStatusChange();
    change.order = OrderContext.fromJson(v.get("order"));
    change.from_status = v.get("from_status").asNullableString();
    change.to_status = v.get("to_status").asNullableString();
    change.occurred_at = v.get("occurred_at").asString();
    return change;
  }
}

/** コンテンツ公開の通知（`on-content-published`） */
export class PublishedContent {
  id: string = "";
  content_type: string = "";
  title: string = "";
  slug: string = "";
  published_at: string = "";
  author_id: string | null = null;
  /** カスタムフィールド（JSON のオブジェクト） */
  fields: JsonValue = JsonValue.object();

  static fromJson(v: JsonValue): PublishedContent {
    const content = new PublishedContent();
    content.id = v.get("id").asString();
    content.content_type = v.get("content_type").asString();
    content.title = v.get("title").asString();
    content.slug = v.get("slug").asString();
    content.published_at = v.get("published_at").asString();
    content.author_id = v.get("author_id").asNullableString();
    const fields = v.get("fields");
    if (fields.isObject()) content.fields = fields;
    return content;
  }
}

/** 公開サイトから投稿されたコメント（`check-comment`。保存する前） */
export class CommentSubmission {
  content_type: string = "";
  content_id: string = "";
  content_title: string = "";
  content_slug: string = "";
  /** 返信先のコメント */
  parent_id: string | null = null;
  author_name: string = "";
  author_email: string = "";
  /** 本文（プレーンテキスト） */
  body: string = "";
  ip: string | null = null;
  user_agent: string | null = null;
  /** ホストの判定（`approved` / `pending`） */
  status: string = "";

  static fromJson(v: JsonValue): CommentSubmission {
    const comment = new CommentSubmission();
    comment.content_type = v.get("content_type").asString();
    comment.content_id = v.get("content_id").asString();
    comment.content_title = v.get("content_title").asString();
    comment.content_slug = v.get("content_slug").asString();
    comment.parent_id = v.get("parent_id").asNullableString();
    comment.author_name = v.get("author_name").asString();
    comment.author_email = v.get("author_email").asString();
    comment.body = v.get("body").asString();
    comment.ip = v.get("ip").asNullableString();
    comment.user_agent = v.get("user_agent").asNullableString();
    comment.status = v.get("status").asString();
    return comment;
  }
}

/**
 * コメントの審査結果（`check-comment`）。`allow` はホストの判定のまま、`hold` は承認待ちに、
 * `spam` はスパムにする（ホストの判定より厳しくだけできる）。投稿そのものを断るときは
 * `writeError(message)` を返す
 */
export class CommentVerdict {
  verdict: string = "allow";
  reason: string = "";

  static allow(): CommentVerdict {
    return new CommentVerdict();
  }

  static hold(reason: string = ""): CommentVerdict {
    const v = new CommentVerdict();
    v.verdict = "hold";
    v.reason = reason;
    return v;
  }

  static spam(reason: string = ""): CommentVerdict {
    const v = new CommentVerdict();
    v.verdict = "spam";
    v.reason = reason;
    return v;
  }

  toJson(): JsonValue {
    const json = JsonValue.object().setString("verdict", this.verdict);
    if (this.reason.length > 0) json.setString("reason", this.reason);
    return json;
  }
}

/** 通知の宛先（`send-notification`）。`type` は `staff`（サイトの運営者）か `person`（特定の人） */
export class Recipient {
  type: string = "staff";
  email: string = "";
  name: string | null = null;

  static fromJson(v: JsonValue): Recipient {
    const recipient = new Recipient();
    recipient.type = v.get("type").asString("staff");
    recipient.email = v.get("email").asString();
    recipient.name = v.get("name").asNullableString();
    return recipient;
  }
}

/** テンプレートから作った、1 つの宛先への通知（`send-notification`） */
export class NotificationMessage {
  /** 通知の種類（`order.placed` / `order.shipped` / `audit.session.create` など） */
  kind: string = "";
  recipient: Recipient = new Recipient();
  subject: string = "";
  /** 本文（プレーンテキスト） */
  body: string = "";
  /** HTML の本文（ホストの `[notifications].html` が有効なとき） */
  html: string | null = null;
  /** テンプレートに渡した値（`order` / `audit` / `site_name` など。JSON のオブジェクト） */
  data: JsonValue = JsonValue.object();

  static fromJson(v: JsonValue): NotificationMessage {
    const message = new NotificationMessage();
    message.kind = v.get("kind").asString();
    message.recipient = Recipient.fromJson(v.get("recipient"));
    message.subject = v.get("subject").asString();
    message.body = v.get("body").asString();
    message.html = v.get("html").asNullableString();
    const data = v.get("data");
    if (data.isObject()) message.data = data;
    return message;
  }
}

/** 通知の配信結果（`send-notification`） */
export enum Delivery {
  /** 送った */
  Sent,
  /** 扱わない通知（種類・宛先）。失敗ではなく、ホストは監査ログにも残さない */
  Skipped,
}
