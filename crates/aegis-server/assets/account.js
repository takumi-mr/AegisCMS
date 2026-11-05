/*
 * Member account pages: log in, sign up and "my page" (profile, order history, reviews).
 * Served at /shop/account.js and loaded by account.html with
 *   <script src="..." data-api="{base}/api/v1/members" data-account-url="{base}/account"
 *           data-login-url="..." data-register-url="..." data-min-password="8" defer></script>
 * The session lives in an HttpOnly cookie that the server sets at login: this script never sees the token.
 * Every request is JSON to the members API; the page is built with textContent only (no HTML from data).
 * The texts come from window.AEGIS_CART_I18N ({ locale, messages }) in the language of [theme].locale.
 * Error messages from the API are shown as they are (English).
 */
(function () {
  'use strict';

  var script = document.currentScript;
  var root = document.getElementById('aegis-account');
  if (!root) return;

  var config = {
    api: (script && script.dataset.api) || '/api/v1/members',
    accountUrl: (script && script.dataset.accountUrl) || '/account',
    loginUrl: (script && script.dataset.loginUrl) || '/account/login',
    registerUrl: (script && script.dataset.registerUrl) || '',
    forgotUrl: (script && script.dataset.forgotUrl) || '',
    minPassword: parseInt((script && script.dataset.minPassword) || '8', 10) || 8,
    view: root.dataset.view || 'home',
  };
  var I18N = window.AEGIS_CART_I18N || { locale: undefined, messages: {} };

  // ---------- helpers ----------

  function msg(key, params) {
    var text = I18N.messages[key];
    if (text === undefined) return key;
    return text.replace(/\{(\w+)\}/g, function (match, name) {
      return params && params[name] !== undefined ? String(params[name]) : match;
    });
  }

  function formatMoney(amount, currency) {
    currency = (currency || 'jpy').toUpperCase();
    try {
      var format = new Intl.NumberFormat(I18N.locale, { style: 'currency', currency: currency });
      var digits = format.resolvedOptions().maximumFractionDigits;
      return format.format(amount / Math.pow(10, digits));
    } catch (e) {
      return currency + ' ' + amount;
    }
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

  // A small element builder. Children are nodes or strings; strings become text nodes.
  function h(tag, attrs, children) {
    var node = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (name) {
      var value = attrs[name];
      if (value === undefined || value === null || value === false) return;
      if (name === 'class') node.className = value;
      else if (name === 'onclick') node.addEventListener('click', value);
      else if (name === 'onsubmit') node.addEventListener('submit', value);
      else node.setAttribute(name, value === true ? '' : value);
    });
    (children || []).forEach(function (child) {
      if (child === null || child === undefined || child === false) return;
      node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
    });
    return node;
  }

  function api(method, path, body) {
    var options = {
      method: method,
      headers: { Accept: 'application/json' },
      credentials: 'same-origin',
    };
    if (body !== undefined) {
      options.headers['Content-Type'] = 'application/json';
      options.body = JSON.stringify(body);
    }
    return fetch(config.api + path, options).then(function (res) {
      if (res.status === 204) return null;
      return res
        .json()
        .catch(function () {
          return {};
        })
        .then(function (data) {
          if (!res.ok) {
            var error = new Error((data && data.error) || msg('request_failed'));
            error.status = res.status;
            error.code = data && data.code;
            throw error;
          }
          return data;
        });
    });
  }

  function field(label, input) {
    return h('label', { class: 'aegis-field' }, [h('span', {}, [label]), input]);
  }

  function input(name, type, extra) {
    var attrs = { name: name, type: type };
    Object.keys(extra || {}).forEach(function (key) {
      attrs[key] = extra[key];
    });
    return h('input', attrs);
  }

  function notice(text, kind) {
    return h('p', { class: kind === 'ok' ? 'aegis-account-ok' : 'aegis-error', role: kind === 'ok' ? 'status' : 'alert' }, [text]);
  }

  // A form that disables its button while the request runs and shows what went wrong
  function form(fields, submitLabel, onSubmit) {
    var status = h('div', { 'aria-live': 'polite' });
    var button = h('button', { type: 'submit', class: 'aegis-button' }, [submitLabel]);
    var node = h('form', { class: 'aegis-account-form', novalidate: true }, fields.concat([button, status]));
    node.addEventListener('submit', function (event) {
      event.preventDefault();
      status.textContent = '';
      var data = {};
      Array.prototype.forEach.call(node.elements, function (element) {
        if (element.name) data[element.name] = element.value;
      });
      button.disabled = true;
      // A check that throws before the request (a short password) is shown like an API error
      Promise.resolve()
        .then(function () {
          return onSubmit(data, status);
        })
        .catch(function (error) {
          status.textContent = '';
          status.appendChild(notice(error.message, 'error'));
        })
        .then(function () {
          button.disabled = false;
        });
    });
    return node;
  }

  function show(children) {
    while (root.firstChild) root.removeChild(root.firstChild);
    children.forEach(function (child) {
      root.appendChild(child);
    });
  }

  function link(href, text) {
    return h('a', { href: href }, [text]);
  }

  // ---------- views ----------

  function loginView() {
    var children = [
      h('h2', {}, [msg('acct_login_title')]),
      form(
        [
          field(msg('acct_email'), input('email', 'email', { required: true, autocomplete: 'email' })),
          field(
            msg('acct_password'),
            input('password', 'password', { required: true, autocomplete: 'current-password' })
          ),
        ],
        msg('acct_login'),
        function (data, status) {
          return api('POST', '/login', { email: data.email, password: data.password })
            .then(function () {
              window.location.href = config.accountUrl;
            })
            .catch(function (error) {
              if (error.code !== 'email_not_verified') throw error;
              // The password was right but the address is not confirmed yet: offer the email again
              status.textContent = '';
              status.appendChild(notice(msg('acct_not_verified'), 'error'));
              var again = h('button', { type: 'button', class: 'aegis-link' }, [msg('acct_resend')]);
              again.addEventListener('click', function () {
                again.disabled = true;
                api('POST', '/verify/resend', { email: data.email })
                  .then(function () {
                    status.textContent = '';
                    status.appendChild(notice(msg('acct_resend_sent'), 'ok'));
                  })
                  .catch(function (resendError) {
                    again.disabled = false;
                    status.appendChild(notice(resendError.message, 'error'));
                  });
              });
              status.appendChild(again);
            });
        }
      ),
    ];
    if (config.forgotUrl) {
      children.push(h('p', { class: 'aegis-muted' }, [link(config.forgotUrl, msg('acct_forgot_link'))]));
    }
    if (config.registerUrl) {
      children.push(h('p', { class: 'aegis-muted' }, [msg('acct_no_account') + ' ', link(config.registerUrl, msg('acct_register'))]));
    }
    show(children);
  }

  // The reset requests answer the same whether or not the address has an account
  function forgotView() {
    show([
      h('h2', {}, [msg('acct_forgot_title')]),
      h('p', { class: 'aegis-muted' }, [msg('acct_forgot_intro')]),
      form(
        [field(msg('acct_email'), input('email', 'email', { required: true, autocomplete: 'email' }))],
        msg('acct_send_link'),
        function (data, status) {
          return api('POST', '/password-reset', { email: data.email }).then(function () {
            status.textContent = '';
            status.appendChild(notice(msg('acct_forgot_sent'), 'ok'));
          });
        }
      ),
      h('p', { class: 'aegis-muted' }, [link(config.loginUrl, msg('acct_login'))]),
    ]);
  }

  // Ask for the confirmation email again (answers the same whether or not the address is waiting)
  function resendForm() {
    return form(
      [field(msg('acct_email'), input('email', 'email', { required: true, autocomplete: 'email' }))],
      msg('acct_resend'),
      function (data, status) {
        return api('POST', '/verify/resend', { email: data.email }).then(function () {
          status.textContent = '';
          status.appendChild(notice(msg('acct_resend_sent'), 'ok'));
        });
      }
    );
  }

  // The link of the confirmation email carries the token: read it, take it out of the address bar, use it once
  function verifyView() {
    var params = new URLSearchParams(window.location.search);
    var token = params.get('token') || '';
    if (token) {
      params.delete('token');
      try {
        var query = params.toString();
        window.history.replaceState(null, '', window.location.pathname + (query ? '?' + query : ''));
      } catch (e) {}
    }
    var failed = function () {
      show([h('h2', {}, [msg('acct_verify_title')]), notice(msg('acct_verify_bad_link'), 'error'), resendForm()]);
    };
    if (!token) {
      failed();
      return;
    }
    show([h('h2', {}, [msg('acct_verify_title')]), h('p', { class: 'aegis-muted' }, [msg('acct_verifying')])]);
    api('POST', '/verify', { token: token })
      .then(function () {
        show([
          h('h2', {}, [msg('acct_verify_title')]),
          notice(msg('acct_verify_done'), 'ok'),
          h('p', {}, [link(config.loginUrl, msg('acct_login'))]),
        ]);
      })
      .catch(failed);
  }

  // The link of the email carries the token: take it out of the address bar once it is read
  function resetView() {
    var params = new URLSearchParams(window.location.search);
    var token = params.get('token') || '';
    if (token) {
      params.delete('token');
      try {
        var query = params.toString();
        window.history.replaceState(null, '', window.location.pathname + (query ? '?' + query : ''));
      } catch (e) {}
    }
    if (!token) {
      show([
        notice(msg('acct_reset_bad_link'), 'error'),
        h('p', {}, [config.forgotUrl ? link(config.forgotUrl, msg('acct_request_new')) : '']),
      ]);
      return;
    }
    show([
      h('h2', {}, [msg('acct_reset_title')]),
      form(
        [
          field(
            msg('acct_new_password') + ' (' + msg('acct_password_hint', { n: config.minPassword }) + ')',
            input('password', 'password', { required: true, minlength: String(config.minPassword), autocomplete: 'new-password' })
          ),
          field(msg('acct_confirm_password'), input('again', 'password', { required: true, autocomplete: 'new-password' })),
        ],
        msg('acct_reset_submit'),
        function (data) {
          if (data.password !== data.again) throw new Error(msg('acct_password_mismatch'));
          return api('POST', '/password-reset/complete', { token: token, password: data.password }).then(function () {
            show([notice(msg('acct_reset_done'), 'ok'), h('p', {}, [link(config.loginUrl, msg('acct_login'))])]);
          });
        }
      ),
    ]);
  }

  function registerView() {
    show([
      h('h2', {}, [msg('acct_register_title')]),
      form(
        [
          field(msg('acct_name'), input('name', 'text', { required: true, autocomplete: 'name' })),
          field(msg('acct_email'), input('email', 'email', { required: true, autocomplete: 'email' })),
          field(
            msg('acct_password') + ' (' + msg('acct_password_hint', { n: config.minPassword }) + ')',
            input('password', 'password', {
              required: true,
              minlength: String(config.minPassword),
              autocomplete: 'new-password',
            })
          ),
        ],
        msg('acct_register'),
        function (data) {
          if (data.password.length < config.minPassword) {
            throw new Error(msg('acct_password_hint', { n: config.minPassword }));
          }
          return api('POST', '/register', { email: data.email, password: data.password, name: data.name }).then(function (result) {
            if (result && result.status === 'verification_sent') {
              // The address has to be confirmed first: no login yet
              show([notice(msg('acct_verify_sent'), 'ok'), h('p', {}, [link(config.loginUrl, msg('acct_login'))])]);
              return null;
            }
            return api('POST', '/login', { email: data.email, password: data.password }).then(function () {
              window.location.href = config.accountUrl;
            });
          });
        }
      ),
      h('p', { class: 'aegis-muted' }, [msg('acct_have_account') + ' ', link(config.loginUrl, msg('acct_login'))]),
    ]);
  }

  function statusLabel(status) {
    var key = 'acct_status_' + status;
    return I18N.messages[key] !== undefined ? msg(key) : status;
  }

  function orderRow(order) {
    var details = h('div', { class: 'aegis-account-order-lines', hidden: true });
    var toggle = h('button', { type: 'button', class: 'aegis-link' }, [msg('acct_details')]);
    var loaded = false;
    toggle.addEventListener('click', function () {
      if (!details.hidden) {
        details.hidden = true;
        toggle.textContent = msg('acct_details');
        return;
      }
      details.hidden = false;
      toggle.textContent = msg('acct_hide');
      if (loaded) return;
      details.textContent = '…';
      api('GET', '/orders/' + encodeURIComponent(order.id))
        .then(function (full) {
          loaded = true;
          details.textContent = '';
          (full.lines || []).forEach(function (line) {
            var title = line.title + (line.variant_title ? ' (' + line.variant_title + ')' : '');
            details.appendChild(
              h('div', { class: 'aegis-sum-row' }, [
                h('span', {}, [title + ' × ' + line.quantity]),
                h('strong', {}, [formatMoney(line.line_total, full.currency)]),
              ])
            );
          });
          (full.adjustments || []).forEach(function (adjustment) {
            details.appendChild(
              h('div', { class: 'aegis-sum-row aegis-muted' }, [
                h('span', {}, [adjustment.label]),
                h('span', {}, [formatMoney(adjustment.amount, full.currency)]),
              ])
            );
          });
        })
        .catch(function (error) {
          details.textContent = '';
          details.appendChild(notice(error.message, 'error'));
        });
    });
    return h('li', { class: 'aegis-account-order' }, [
      h('div', { class: 'aegis-order-head' }, [
        h('strong', {}, [msg('acct_order_number', { number: order.number || order.id })]),
        h('span', { class: 'aegis-status aegis-status-' + order.status }, [statusLabel(order.status)]),
      ]),
      h('div', { class: 'aegis-sum-row' }, [
        h('span', { class: 'aegis-muted' }, [formatDate(order.created_at)]),
        h('strong', {}, [msg('acct_total') + ' ' + formatMoney(order.total, order.currency)]),
      ]),
      toggle,
      details,
    ]);
  }

  function reviewRow(review) {
    var stars = '★★★★★'.slice(0, Math.max(0, Math.min(5, review.rating))) + '☆☆☆☆☆'.slice(0, 5 - Math.max(0, Math.min(5, review.rating)));
    return h('li', { class: 'aegis-account-review' }, [
      h('div', { class: 'aegis-order-head' }, [
        h('strong', {}, [stars + (review.title ? ' ' + review.title : '')]),
        h('span', { class: 'aegis-status' }, [I18N.messages['acct_review_' + review.status] !== undefined ? msg('acct_review_' + review.status) : review.status]),
      ]),
      h('p', {}, [review.body]),
      h('span', { class: 'aegis-muted' }, [formatDate(review.created_at)]),
    ]);
  }

  function profileSection(member) {
    var panel = h('div', {}, []);
    var profile = form(
      [
        field(msg('acct_name'), input('name', 'text', { required: true, value: member.name || '', autocomplete: 'name' })),
        field(msg('acct_email'), input('email', 'email', { value: member.email, disabled: true })),
        field(
          msg('acct_nickname') + ' (' + msg('acct_nickname_hint') + ')',
          input('nickname', 'text', { value: member.nickname || '', maxlength: '30', autocomplete: 'nickname' })
        ),
        field(msg('acct_phone'), input('phone', 'tel', { value: member.phone || '', autocomplete: 'tel' })),
      ],
      msg('acct_save'),
      function (data, status) {
        return api('PUT', '/me', { name: data.name, nickname: data.nickname || '', phone: data.phone || null }).then(function () {
          status.textContent = '';
          status.appendChild(notice(msg('acct_saved'), 'ok'));
        });
      }
    );
    var password = form(
      [
        field(msg('acct_current_password'), input('current', 'password', { required: true, autocomplete: 'current-password' })),
        field(
          msg('acct_new_password') + ' (' + msg('acct_password_hint', { n: config.minPassword }) + ')',
          input('next', 'password', { required: true, minlength: String(config.minPassword), autocomplete: 'new-password' })
        ),
      ],
      msg('acct_change_password'),
      function (data, status) {
        return api('PUT', '/me', { current_password: data.current, new_password: data.next }).then(function () {
          status.textContent = '';
          status.appendChild(notice(msg('acct_saved'), 'ok'));
        });
      }
    );
    panel.appendChild(profile);
    panel.appendChild(h('h3', {}, [msg('acct_change_password')]));
    panel.appendChild(password);
    return h('section', {}, [h('h2', {}, [msg('acct_profile')]), panel]);
  }

  // 退会: パスワードで本人を確かめ、確認のダイアログを通ったときだけ送る
  function deleteSection() {
    return h('section', {}, [
      h('h2', {}, [msg('acct_delete_title')]),
      h('p', { class: 'aegis-muted' }, [msg('acct_delete_intro')]),
      form(
        [field(msg('acct_delete_password'), input('password', 'password', { required: true, autocomplete: 'current-password' }))],
        msg('acct_delete_button'),
        function (data) {
          if (!window.confirm(msg('acct_delete_confirm'))) return null;
          return api('DELETE', '/me', { password: data.password }).then(function () {
            window.location.href = config.loginUrl;
          });
        }
      ),
    ]);
  }

  function homeView() {
    show([h('p', { class: 'aegis-muted' }, [msg('acct_loading')])]);
    Promise.all([api('GET', '/me'), api('GET', '/orders'), api('GET', '/reviews')])
      .then(function (results) {
        var member = results[0];
        var orders = results[1].orders || [];
        var reviews = results[2].reviews || [];
        var logout = h('button', { type: 'button', class: 'aegis-button aegis-secondary' }, [msg('acct_logout')]);
        logout.addEventListener('click', function () {
          logout.disabled = true;
          api('POST', '/logout')
            .catch(function () {})
            .then(function () {
              window.location.href = config.loginUrl;
            });
        });

        show([
          h('div', { class: 'aegis-order-head' }, [h('p', {}, [msg('acct_welcome', { name: member.name })]), logout]),
          h('section', {}, [
            h('h2', {}, [msg('acct_orders')]),
            orders.length
              ? h('ul', { class: 'aegis-account-list' }, orders.map(orderRow))
              : h('p', { class: 'aegis-muted' }, [msg('acct_no_orders')]),
          ]),
          h('section', {}, [
            h('h2', {}, [msg('acct_reviews')]),
            reviews.length
              ? h('ul', { class: 'aegis-account-list' }, reviews.map(reviewRow))
              : h('p', { class: 'aegis-muted' }, [msg('acct_no_reviews')]),
          ]),
          profileSection(member),
          deleteSection(),
        ]);
      })
      .catch(function (error) {
        if (error.status === 401) {
          window.location.href = config.loginUrl;
          return;
        }
        show([notice(error.message, 'error')]);
      });
  }

  if (config.view === 'login') loginView();
  else if (config.view === 'register') registerView();
  else if (config.view === 'forgot') forgotView();
  else if (config.view === 'reset') resetView();
  else if (config.view === 'verify') verifyView();
  else homeView();
})();
