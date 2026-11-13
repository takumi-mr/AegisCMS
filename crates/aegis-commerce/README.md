# aegis-commerce

**English** | [日本語](README.ja.md)

AegisCMS's crate for e-commerce, quotes, the order lifecycle, inventory management, and payment integration (`[commerce]`).

## Overview

`aegis-commerce` is an e-commerce subsystem that provides product variant management, cart quotes, order confirmation and state transitions, exclusive locking and reservation of inventory, and processing of online payments (Stripe) and manual payments (bank transfer, cash on delivery).

It is integrated with Wasm plugin hooks for flexible price, tax, and discount rule calculation (`calculate_order`) and for order status changes (`on_order_status_changed`).

## Responsibilities and Features

- **Quotes and price calculation (`CommerceService::quote`)**:
  - Subtotal calculation for the items (variants) in the cart.
  - Dynamic injection of coupon discounts, shipping fees, reduced tax rates, etc., by Wasm plugins (`calculate_order`).
  - Rounding and formatting according to the currency unit.
- **Order lifecycle and inventory management (`orders`)**:
  - A strict state machine for order creation (`pending`), payment completion (`paid`), shipment (`fulfilled`), cancellation (`cancelled`), and refund (`refunded`).
  - Atomic inventory decrement at order creation and automatic inventory restoration on cancellation.
- **Payment provider integration (`payment`)**:
  - **Stripe (`stripe`)**: creating Stripe Checkout Sessions, verifying Stripe webhook signatures, and reflecting statuses automatically.
  - **Manual**: manual reconciliation flows such as bank transfer, invoice payment, and cash on delivery.
- **Member order history and product review management (`member_data`, `review`)**:
  - Viewing a member's order history, and cleanup on withdrawal (detaching orders from the member and anonymizing reviews). The member account itself is handled by `aegis-members`.
  - Posting and moderating purchaser-only ratings and product reviews.

## Module Layout

| Module | Description |
| :--- | :--- |
| `lib` | `CommerceService` implementation; quote and order state machine |
| `payment` | Payment provider abstraction (`PaymentProvider`) |
| `stripe` | Stripe API integration and webhook handling |
| `member_data` | Member order history and cleanup on withdrawal (the members crate does not know about orders, so this crate links them) |
| `review` | Product review management |
| `config` | E-commerce settings (`CommerceConfig`: currency, shipping fees, payment keys, etc.) |
