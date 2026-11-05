/*
 * Product reviews on the product page: the average and the approved reviews, and a form to write one.
 * Served at /shop/reviews.js and loaded by reviews.html with
 *   <script src="..." data-api="{base}/api/v1/products" data-members-api="{base}/api/v1/members"
 *           data-product-id="..." data-login-url="..." data-register-url="..." defer></script>
 * Who may write: a logged-in member (the HttpOnly cookie of the account pages) for one of their paid
 * orders with this product, or a guest who opens the link of their order page
 * (?review_order=ID&token=TOKEN), which this script removes from the address bar as soon as it has read it.
 * The server checks the purchase and the one-review-per-order rule; the page is built with textContent only.
 * The texts come from window.AEGIS_CART_I18N ({ locale, messages }) in the language of [theme].locale.
 */
(function () {
  'use strict';

  var script = document.currentScript;
  var root = document.getElementById('aegis-reviews');
  if (!root || !script) return;

  var config = {
    api: script.dataset.api || '/api/v1/products',
    membersApi: script.dataset.membersApi || '/api/v1/members',
    productId: script.dataset.productId || '',
    loginUrl: script.dataset.loginUrl || '',
    registerUrl: script.dataset.registerUrl || '',
  };
  var I18N = window.AEGIS_CART_I18N || { locale: undefined, messages: {} };
  // Statuses of an order that can be reviewed (the server checks the same list)
  var REVIEWABLE = /^(paid|processing|shipped|delivered|completed)$/;

  // The guest link of the order page: read it once and take the token out of the address bar
  var guest = (function () {
    var params = new URLSearchParams(window.location.search);
    var order = params.get('review_order');
    var token = params.get('token');
    if (!order && !token) return null;
    params.delete('review_order');
    params.delete('token');
    try {
      var query = params.toString();
      window.history.replaceState(null, '', window.location.pathname + (query ? '?' + query : '') + window.location.hash);
    } catch (e) {}
    return order && token ? { order: order, token: token } : null;
  })();

  // ---------- helpers ----------

  function msg(key, params) {
    var text = I18N.messages[key];
    if (text === undefined) return key;
    return text.replace(/\{(\w+)\}/g, function (match, name) {
      return params && params[name] !== undefined ? String(params[name]) : match;
    });
  }

  function formatDate(iso) {
    var date = new Date(iso);
    if (isNaN(date.getTime())) return '';
    try {
      return date.toLocaleDateString(I18N.locale);
    } catch (e) {
      return date.toISOString().slice(0, 10);
    }
  }

  function stars(rating) {
    var n = Math.max(0, Math.min(5, Math.round(rating)));
    return '★★★★★'.slice(0, n) + '☆☆☆☆☆'.slice(0, 5 - n);
  }

  function h(tag, attrs, children) {
    var node = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (name) {
      var value = attrs[name];
      if (value === undefined || value === null || value === false) return;
      if (name === 'class') node.className = value;
      else node.setAttribute(name, value === true ? '' : value);
    });
    (children || []).forEach(function (child) {
      if (child === null || child === undefined || child === false) return;
      node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
    });
    return node;
  }

  function request(method, url, body) {
    var options = { method: method, headers: { Accept: 'application/json' }, credentials: 'same-origin' };
    if (body !== undefined) {
      options.headers['Content-Type'] = 'application/json';
      options.body = JSON.stringify(body);
    }
    return fetch(url, options).then(function (res) {
      return res
        .json()
        .catch(function () {
          return {};
        })
        .then(function (data) {
          if (!res.ok) {
            var error = new Error((data && data.error) || msg('request_failed'));
            error.status = res.status;
            throw error;
          }
          return data;
        });
    });
  }

  // ---------- the list ----------

  var listBox = h('div', { class: 'aegis-reviews-list' });
  var formBox = h('div', { class: 'aegis-reviews-form' });

  function reviewItem(review) {
    var head = [h('strong', { class: 'aegis-review-stars', 'aria-label': msg('rev_stars', { n: review.rating }) }, [stars(review.rating)])];
    if (review.title) head.push(h('strong', {}, [review.title]));
    // 退会した顧客のレビューは、名前が空（匿名にして残してある）
    var meta = [review.author_name || msg('rev_deleted_author'), formatDate(review.created_at)];
    var children = [
      h('div', { class: 'aegis-review-head' }, head),
      h('p', { class: 'aegis-review-body' }, [review.body]),
      h('div', { class: 'aegis-muted' }, [
        meta.filter(Boolean).join(' · '),
        review.verified_purchase ? h('span', { class: 'aegis-review-badge' }, [msg('rev_verified')]) : null,
      ]),
    ];
    if (review.reply) {
      children.push(
        h('div', { class: 'aegis-review-reply' }, [h('strong', {}, [msg('rev_reply')]), h('p', {}, [review.reply])])
      );
    }
    return h('li', { class: 'aegis-review' }, children);
  }

  function loadList() {
    return request('GET', config.api + '/' + encodeURIComponent(config.productId) + '/reviews?limit=50')
      .then(function (data) {
        var reviews = data.reviews || [];
        var stats = data.stats || {};
        listBox.textContent = '';
        if (!reviews.length) {
          listBox.appendChild(h('p', { class: 'aegis-muted' }, [msg('rev_none')]));
          return;
        }
        listBox.appendChild(
          h('p', { class: 'aegis-review-summary' }, [
            h('strong', {}, [stars(stats.average_rating || 0)]),
            ' ' + (stats.average_rating || 0).toFixed(1) + ' / 5 (' + msg('rev_count', { n: stats.review_count || reviews.length }) + ')',
          ])
        );
        listBox.appendChild(h('ul', { class: 'aegis-reviews-ul' }, reviews.map(reviewItem)));
      })
      .catch(function (error) {
        listBox.textContent = '';
        listBox.appendChild(h('p', { class: 'aegis-error', role: 'alert' }, [error.message]));
      });
  }

  // ---------- the form ----------

  function loginHint() {
    var children = [msg('rev_login_needed') + ' '];
    if (config.loginUrl) children.push(h('a', { href: config.loginUrl }, [msg('rev_login')]));
    return h('p', { class: 'aegis-muted' }, children);
  }

  function field(label, control) {
    return h('label', { class: 'aegis-field' }, [h('span', {}, [label]), control]);
  }

  // `orders`: [{ id, label }] the member may review (empty for a guest, who has one order from the link)
  function writeForm(orders, asGuest) {
    var ratingSelect = h('select', { name: 'rating', required: true }, [5, 4, 3, 2, 1].map(function (n) {
      return h('option', { value: String(n) }, [stars(n) + ' (' + n + ')']);
    }));
    var fields = [];
    if (asGuest) fields.push(field(msg('rev_name'), h('input', { name: 'author_name', type: 'text', required: true, maxlength: '100' })));
    if (!asGuest && orders.length > 1) {
      fields.push(
        field(
          msg('rev_order'),
          h('select', { name: 'order_id', required: true }, orders.map(function (order) {
            return h('option', { value: order.id }, [order.label]);
          }))
        )
      );
    }
    fields.push(field(msg('rev_rating'), ratingSelect));
    fields.push(field(msg('rev_title'), h('input', { name: 'title', type: 'text', required: true, maxlength: '200' })));
    fields.push(field(msg('rev_body'), h('textarea', { name: 'body', required: true, rows: '5', maxlength: '5000' })));

    var status = h('div', { 'aria-live': 'polite' });
    var button = h('button', { type: 'submit', class: 'aegis-button' }, [msg('rev_submit')]);
    var form = h('form', { class: 'aegis-account-form', novalidate: true }, fields.concat([button, status]));

    form.addEventListener('submit', function (event) {
      event.preventDefault();
      var data = {};
      Array.prototype.forEach.call(form.elements, function (element) {
        if (element.name) data[element.name] = element.value;
      });
      var payload = {
        order_id: asGuest ? guest.order : data.order_id || orders[0].id,
        rating: parseInt(data.rating, 10),
        title: data.title,
        body: data.body,
      };
      if (asGuest) {
        payload.access_token = guest.token;
        payload.author_name = data.author_name;
      }
      status.textContent = '';
      button.disabled = true;
      request('POST', config.api + '/' + encodeURIComponent(config.productId) + '/reviews', payload)
        .then(function (result) {
          var published = result.status === 'approved';
          formBox.textContent = '';
          formBox.appendChild(h('p', { class: 'aegis-account-ok', role: 'status' }, [msg(published ? 'rev_thanks' : 'rev_pending')]));
          return loadList();
        })
        .catch(function (error) {
          status.textContent = '';
          status.appendChild(h('p', { class: 'aegis-error', role: 'alert' }, [error.message]));
          button.disabled = false;
        });
    });
    return form;
  }

  function loadForm() {
    if (guest) {
      formBox.appendChild(h('h3', {}, [msg('rev_write')]));
      formBox.appendChild(writeForm([], true));
      return Promise.resolve();
    }
    // A logged-in member: the orders that have this product and can be reviewed
    return request('GET', config.membersApi + '/orders')
      .then(function (data) {
        var orders = (data.orders || [])
          .filter(function (order) {
            return (
              REVIEWABLE.test(order.status) &&
              (order.lines || []).some(function (line) {
                return line.product_id === config.productId;
              })
            );
          })
          .map(function (order) {
            return { id: order.id, label: (order.number || order.id) + ' (' + formatDate(order.created_at) + ')' };
          });
        // One review per order and product: the ones already written are taken out
        return request('GET', config.membersApi + '/reviews').then(function (mine) {
          var written = {};
          (mine.reviews || []).forEach(function (review) {
            if (review.product_id === config.productId) written[review.order_id] = true;
          });
          return orders.filter(function (order) {
            return !written[order.id];
          });
        });
      })
      .then(function (orders) {
        if (!orders.length) {
          formBox.appendChild(h('p', { class: 'aegis-muted' }, [msg('rev_buy_first')]));
          return;
        }
        formBox.appendChild(h('h3', {}, [msg('rev_write')]));
        formBox.appendChild(writeForm(orders, false));
      })
      .catch(function (error) {
        if (error.status === 401) {
          formBox.appendChild(loginHint());
          return;
        }
        formBox.appendChild(h('p', { class: 'aegis-error', role: 'alert' }, [error.message]));
      });
  }

  root.textContent = '';
  root.appendChild(listBox);
  root.appendChild(formBox);
  loadList();
  loadForm();
})();
