/*
 * AegisCMS shop: cart (localStorage), checkout and the order page.
 * Served at /shop/cart.js and loaded by the shop templates with
 *   <script src="..." data-api="{base}/api/v1/shop" data-currency="jpy"
 *           data-cart-url="{base}/cart" data-products-url="{base}/products" defer></script>
 * Prices shown before checkout come from the server's quote; the browser never decides them.
 */
(function () {
  'use strict';

  var script = document.currentScript;
  var config = {
    api: (script && script.dataset.api) || '/api/v1/shop',
    currency: (script && script.dataset.currency) || 'jpy',
    cartUrl: (script && script.dataset.cartUrl) || '/cart',
    productsUrl: (script && script.dataset.productsUrl) || '/products',
    shopEnabled: !script || script.dataset.shopEnabled !== 'false',
  };
  var STORAGE_KEY = 'aegis.cart.v1';
  var memoryCart = [];

  // ---------- helpers ----------

  function el(tag, attrs, children) {
    var node = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (key) {
      var value = attrs[key];
      if (value === undefined || value === null || value === false) return;
      if (key === 'text') node.textContent = String(value);
      else if (key === 'className') node.className = value;
      else if (key.indexOf('on') === 0) node.addEventListener(key.slice(2), value);
      else node.setAttribute(key, value === true ? '' : String(value));
    });
    (children || []).forEach(function (child) {
      if (child) node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
    });
    return node;
  }

  function formatMoney(amount, currency) {
    currency = (currency || config.currency).toUpperCase();
    try {
      var format = new Intl.NumberFormat(undefined, { style: 'currency', currency: currency });
      var digits = format.resolvedOptions().maximumFractionDigits;
      return format.format(amount / Math.pow(10, digits));
    } catch (e) {
      return currency + ' ' + amount;
    }
  }

  function request(path, options) {
    return fetch(config.api + path, options).then(function (res) {
      return res
        .json()
        .catch(function () {
          return {};
        })
        .then(function (body) {
          if (!res.ok) {
            var error = new Error(body.error || 'Request failed (' + res.status + ')');
            error.status = res.status;
            error.body = body;
            throw error;
          }
          return body;
        });
    });
  }

  // ---------- cart storage ----------

  function loadCart() {
    try {
      var stored = JSON.parse(localStorage.getItem(STORAGE_KEY) || '[]');
      return Array.isArray(stored) ? stored : [];
    } catch (e) {
      return memoryCart.slice();
    }
  }

  function saveCart(items) {
    memoryCart = items.slice();
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
    } catch (e) {
      /* storage unavailable: keep the cart for this page only */
    }
    updateCounts();
  }

  function cartCount() {
    return loadCart().reduce(function (sum, item) {
      return sum + item.quantity;
    }, 0);
  }

  function updateCounts() {
    var count = cartCount();
    document.querySelectorAll('[data-aegis-cart-count]').forEach(function (node) {
      node.textContent = String(count);
      node.hidden = count === 0;
    });
  }

  function addToCart(product, quantity) {
    var items = loadCart();
    var existing = items.find(function (item) {
      return item.product === product.product;
    });
    if (existing) existing.quantity += quantity;
    else items.push(Object.assign({}, product, { quantity: quantity }));
    saveCart(items);
  }

  // ---------- product pages ----------

  function bindAddToCart() {
    document.querySelectorAll('[data-aegis-add-to-cart]').forEach(function (button) {
      button.addEventListener('click', function () {
        var input = button.dataset.quantityInput && document.querySelector(button.dataset.quantityInput);
        var quantity = Math.max(1, parseInt((input && input.value) || '1', 10) || 1);
        addToCart(
          {
            product: button.dataset.productId,
            slug: button.dataset.productSlug,
            title: button.dataset.productTitle,
            price: parseInt(button.dataset.productPrice || '0', 10),
          },
          quantity
        );
        var label = button.textContent;
        button.textContent = button.dataset.addedLabel || 'Added to cart ✓';
        button.disabled = true;
        setTimeout(function () {
          button.textContent = label;
          button.disabled = false;
        }, 1400);
      });
    });
  }

  // ---------- cart page ----------

  function renderCart(root) {
    var state = { coupon: '', quote: null, error: null, busy: false, timer: null };

    function cartItems() {
      return loadCart().map(function (item) {
        return { product: item.product, quantity: item.quantity };
      });
    }

    function requote() {
      var items = cartItems();
      if (!items.length || !config.shopEnabled) {
        state.quote = null;
        draw();
        return;
      }
      request('/quote', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ items: items, coupon_code: state.coupon || null }),
      })
        .then(function (quote) {
          state.quote = quote;
          state.error = null;
        })
        .catch(function (err) {
          state.quote = null;
          state.error = err.message;
        })
        .then(draw);
    }

    function scheduleRequote() {
      clearTimeout(state.timer);
      state.timer = setTimeout(requote, 350);
    }

    function setQuantity(index, quantity) {
      var items = loadCart();
      if (quantity <= 0) items.splice(index, 1);
      else items[index].quantity = quantity;
      saveCart(items);
      draw();
      scheduleRequote();
    }

    function checkout(form) {
      var data = new FormData(form);
      var value = function (name) {
        return String(data.get(name) || '').trim();
      };
      state.busy = true;
      state.error = null;
      draw();
      request('/checkout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          items: cartItems(),
          email: value('email'),
          name: value('name'),
          coupon_code: state.coupon || null,
          shipping_address: {
            name: value('name'),
            postal_code: value('postal_code'),
            region: value('region'),
            city: value('city'),
            line1: value('line1'),
            line2: value('line2'),
            phone: value('phone'),
            country: value('country') || 'JP',
          },
        }),
      })
        .then(function (result) {
          // The cart is emptied on the order page, so it survives a cancelled payment
          window.location.href = result.redirect_url || result.order_url;
        })
        .catch(function (err) {
          state.busy = false;
          state.error = err.message;
          draw();
          if (err.status === 409) requote();
        });
    }

    function summaryRows() {
      var quote = state.quote;
      if (!quote) return [];
      var rows = [el('div', { className: 'aegis-sum-row' }, [el('span', { text: 'Subtotal' }), el('span', { text: formatMoney(quote.subtotal, quote.currency) })])];
      quote.adjustments.forEach(function (adjustment) {
        rows.push(
          el('div', { className: 'aegis-sum-row' + (adjustment.amount < 0 ? ' aegis-discount' : '') }, [
            el('span', { text: adjustment.label }),
            el('span', { text: formatMoney(adjustment.amount, quote.currency) }),
          ])
        );
      });
      rows.push(el('div', { className: 'aegis-sum-row aegis-total' }, [el('span', { text: 'Total' }), el('strong', { text: formatMoney(quote.total, quote.currency) })]));
      return rows;
    }

    function field(name, label, attrs) {
      return el('label', { className: 'aegis-field' }, [
        el('span', { text: label }),
        el('input', Object.assign({ name: name, autocomplete: (attrs && attrs.autocomplete) || 'on' }, attrs || {})),
      ]);
    }

    function draw() {
      var items = loadCart();
      root.textContent = '';
      if (!items.length) {
        root.appendChild(
          el('div', { className: 'aegis-empty' }, [
            el('p', { text: 'Your cart is empty.' }),
            el('a', { href: config.productsUrl, className: 'aegis-button', text: 'Browse products' }),
          ])
        );
        return;
      }

      var list = el('div', { className: 'aegis-cart-lines' });
      items.forEach(function (item, index) {
        var line = state.quote && state.quote.lines.find(function (l) {
          return l.product_id === item.product || l.slug === item.slug;
        });
        var price = line ? line.unit_price : item.price;
        list.appendChild(
          el('div', { className: 'aegis-cart-line' }, [
            el('a', { href: config.productsUrl + '/' + encodeURIComponent(item.slug || item.product), className: 'aegis-line-title', text: item.title || item.slug }),
            el('span', { className: 'aegis-muted', text: formatMoney(price) }),
            el('input', {
              type: 'number',
              min: '1',
              value: String(item.quantity),
              'aria-label': 'Quantity of ' + (item.title || item.slug),
              className: 'aegis-qty',
              onchange: function (e) {
                setQuantity(index, parseInt(e.target.value, 10) || 0);
              },
            }),
            el('strong', { text: formatMoney(price * item.quantity) }),
            el('button', { type: 'button', className: 'aegis-link', text: 'Remove', onclick: function () { setQuantity(index, 0); } }),
          ])
        );
      });
      root.appendChild(list);

      if (!config.shopEnabled) {
        root.appendChild(el('p', { className: 'aegis-error', text: 'Online checkout is not available on this site.' }));
        return;
      }

      var coupon = el('input', { name: 'coupon', placeholder: 'Coupon code', value: state.coupon, 'aria-label': 'Coupon code' });
      root.appendChild(
        el('div', { className: 'aegis-coupon' }, [
          coupon,
          el('button', {
            type: 'button',
            className: 'aegis-button aegis-secondary',
            text: 'Apply',
            onclick: function () {
              state.coupon = coupon.value.trim();
              requote();
            },
          }),
        ])
      );
      root.appendChild(el('div', { className: 'aegis-summary' }, summaryRows()));
      if (state.error) root.appendChild(el('p', { className: 'aegis-error', role: 'alert', text: state.error }));

      var form = el(
        'form',
        {
          className: 'aegis-checkout',
          onsubmit: function (e) {
            e.preventDefault();
            checkout(e.target);
          },
        },
        [
          el('h2', { text: 'Checkout' }),
          field('name', 'Name', { required: true, autocomplete: 'name' }),
          field('email', 'Email', { type: 'email', required: true, autocomplete: 'email' }),
          field('postal_code', 'Postal code', { autocomplete: 'postal-code' }),
          field('region', 'Prefecture / State', { autocomplete: 'address-level1' }),
          field('city', 'City', { autocomplete: 'address-level2' }),
          field('line1', 'Address', { autocomplete: 'address-line1' }),
          field('line2', 'Building / Room', { autocomplete: 'address-line2' }),
          field('phone', 'Phone', { type: 'tel', autocomplete: 'tel' }),
          el('button', {
            type: 'submit',
            className: 'aegis-button',
            disabled: state.busy || !state.quote,
            text: state.busy ? 'Placing order…' : 'Place order',
          }),
        ]
      );
      root.appendChild(form);
    }

    draw();
    requote();
  }

  // ---------- order page ----------

  var STATUS_LABELS = {
    pending_payment: 'Awaiting payment',
    paid: 'Paid',
    shipped: 'Shipped',
    completed: 'Completed',
    cancelled: 'Cancelled',
  };

  function renderOrder(root) {
    var id = root.dataset.orderId;
    var token = new URLSearchParams(window.location.search).get('token') || '';
    var attempts = 0;

    function draw(order) {
      root.textContent = '';
      root.appendChild(
        el('div', { className: 'aegis-order-head' }, [
          el('div', {}, [el('div', { className: 'aegis-muted', text: 'Order number' }), el('h2', { text: order.number })]),
          el('span', { className: 'aegis-status aegis-status-' + order.status, text: STATUS_LABELS[order.status] || order.status }),
        ])
      );
      if (order.status === 'pending_payment' && order.payment_provider === 'stripe') {
        root.appendChild(el('p', { className: 'aegis-muted', text: 'Confirming your payment… this page updates automatically.' }));
      }
      if (order.payment_instructions) {
        root.appendChild(el('div', { className: 'aegis-instructions' }, [el('strong', { text: 'How to pay' }), el('p', { text: order.payment_instructions })]));
      }
      var lines = el('div', { className: 'aegis-summary' });
      order.lines.forEach(function (line) {
        lines.appendChild(el('div', { className: 'aegis-sum-row' }, [el('span', { text: line.title + ' × ' + line.quantity }), el('span', { text: formatMoney(line.line_total, order.currency) })]));
      });
      order.adjustments.forEach(function (adjustment) {
        lines.appendChild(el('div', { className: 'aegis-sum-row' }, [el('span', { text: adjustment.label }), el('span', { text: formatMoney(adjustment.amount, order.currency) })]));
      });
      lines.appendChild(el('div', { className: 'aegis-sum-row aegis-total' }, [el('span', { text: 'Total' }), el('strong', { text: formatMoney(order.total, order.currency) })]));
      root.appendChild(lines);
      var address = order.shipping_address;
      if (address && (address.line1 || address.city)) {
        root.appendChild(
          el('div', { className: 'aegis-address' }, [
            el('strong', { text: 'Ship to' }),
            el('p', { text: [address.name, address.postal_code, address.region + ' ' + address.city, address.line1, address.line2].filter(function (s) { return s && s.trim(); }).join('\n') }),
          ])
        );
      }
      root.appendChild(el('a', { href: config.productsUrl, className: 'aegis-button aegis-secondary', text: 'Continue shopping' }));
    }

    function load() {
      request('/orders/' + encodeURIComponent(id) + '?token=' + encodeURIComponent(token))
        .then(function (order) {
          if (order.status !== 'cancelled') saveCart([]);
          draw(order);
          // Stripe confirms the payment by webhook, usually within seconds
          if (order.status === 'pending_payment' && order.payment_provider === 'stripe' && attempts++ < 20) {
            setTimeout(load, 3000);
          }
        })
        .catch(function (err) {
          root.textContent = '';
          root.appendChild(el('p', { className: 'aegis-error', text: err.status === 404 ? 'Order not found. Use the link from your confirmation.' : err.message }));
        });
    }
    load();
  }

  function init() {
    updateCounts();
    bindAddToCart();
    var cart = document.getElementById('aegis-cart');
    if (cart) renderCart(cart);
    var order = document.getElementById('aegis-order');
    if (order) renderOrder(order);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  window.AegisCart = { add: addToCart, items: loadCart, clear: function () { saveCart([]); } };
})();
