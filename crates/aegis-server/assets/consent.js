/*
 * AegisCMS cookie consent. Served at /_aegis/consent.js when [consent].enabled = true.
 *
 * The banner's HTML comes from the theme's consent.html (a built-in one when the theme has none); this
 * script finds its parts by data attributes and makes them work:
 *   [data-aegis-consent-root]               the dialog (hidden until a choice is needed)
 *   [data-aegis-consent-action="..."]       accept-all | reject-all | customize | save
 *   [data-aegis-consent-details]            the per-category choices (shown by "customize"; hidden at first)
 *   input[data-aegis-consent-category="id"] one checkbox per category
 *   [data-aegis-consent-reopen]             the button that reopens the settings (hidden until a choice exists)
 *   [data-aegis-consent-open]               any link or button elsewhere on the page that reopens them
 * The root gets data-aegis-consent-state="ask" or "details" while it is open, for CSS.
 *
 * - Settings: <script type="application/json" id="aegis-consent-config">.
 * - The choice is stored in the first-party cookie "aegis_consent": {"r": revision, "c": {category: bool}}.
 *   A different revision (the categories changed) asks again.
 * - Scripts and iframes that need consent wait until it is given:
 *     <script type="text/plain" data-aegis-consent="marketing" src="https://..."></script>
 *     <iframe data-aegis-consent="marketing" data-src="https://..."></iframe>
 * - window.AegisConsent: has(id), get(), open(), onChange(fn), destroy(). Event "aegis:consent" on document.
 */
(function () {
  'use strict';
  if (window.AegisConsent) return;

  var COOKIE = 'aegis_consent';
  var configElement = document.getElementById('aegis-consent-config');
  if (!configElement) return;
  var config;
  try {
    config = JSON.parse(configElement.textContent || '');
  } catch (e) {
    return;
  }
  var categories = config.categories || [];
  var listeners = [];
  var state = readCookie();
  var root = null;
  var reopen = null;
  var details = null;
  var lastFocus = null;
  var timer = null;
  var open = false;

  // --- cookie ---

  function readCookie() {
    var match = document.cookie.split('; ').filter(function (part) {
      return part.indexOf(COOKIE + '=') === 0;
    })[0];
    if (!match) return null;
    try {
      var value = JSON.parse(decodeURIComponent(match.slice(COOKIE.length + 1)));
      if (!value || value.r !== config.rev || typeof value.c !== 'object' || !value.c) return null;
      return value;
    } catch (e) {
      return null;
    }
  }

  function writeCookie(choices) {
    var value = encodeURIComponent(JSON.stringify({ r: config.rev, c: choices }));
    var secure = location.protocol === 'https:' ? '; Secure' : '';
    document.cookie =
      COOKIE + '=' + value + '; Max-Age=' + config.days * 86400 + '; Path=/; SameSite=Lax' + secure;
  }

  // --- state ---

  function granted(id) {
    return !!(state && state.c && state.c[id] === true);
  }

  function snapshot() {
    var result = {};
    categories.forEach(function (id) {
      result[id] = granted(id);
    });
    return result;
  }

  function notify() {
    var choices = snapshot();
    listeners.slice().forEach(function (fn) {
      try {
        fn(choices);
      } catch (e) {
        /* a listener must not stop the others */
      }
    });
    document.dispatchEvent(new CustomEvent('aegis:consent', { detail: { categories: choices } }));
  }

  /** Runs the scripts / iframes that were waiting for a category that is now granted. */
  function activate() {
    var waiting = document.querySelectorAll('[data-aegis-consent]:not([data-aegis-consent-done])');
    Array.prototype.forEach.call(waiting, function (element) {
      if (!granted(element.getAttribute('data-aegis-consent'))) return;
      element.setAttribute('data-aegis-consent-done', '');
      if (element.tagName === 'SCRIPT') {
        var script = document.createElement('script');
        Array.prototype.forEach.call(element.attributes, function (attribute) {
          if (attribute.name !== 'type' && attribute.name !== 'data-aegis-consent-done') {
            script.setAttribute(attribute.name, attribute.value);
          }
        });
        script.text = element.text;
        element.parentNode.replaceChild(script, element);
      } else if (element.hasAttribute('data-src')) {
        element.setAttribute('src', element.getAttribute('data-src'));
      }
    });
  }

  function choicesFrom(value) {
    var choices = {};
    categories.forEach(function (id) {
      choices[id] = value;
    });
    return choices;
  }

  function save(choices) {
    var revoked = categories.some(function (id) {
      return granted(id) && choices[id] !== true;
    });
    state = { r: config.rev, c: choices };
    writeCookie(choices);
    hide();
    // What already ran cannot be taken back: start over without it
    if (revoked) {
      location.reload();
      return;
    }
    activate();
    notify();
  }

  // --- the banner (markup from the theme) ---

  function part(name) {
    return root ? root.querySelector('[data-aegis-consent-action="' + name + '"]') : null;
  }

  function checkboxes() {
    return root ? root.querySelectorAll('input[data-aegis-consent-category]') : [];
  }

  function focusable() {
    return Array.prototype.filter.call(
      root.querySelectorAll('button, a[href], input, select, textarea, [tabindex]'),
      function (node) {
        return !node.disabled && !node.hidden && node.getAttribute('tabindex') !== '-1' && node.offsetParent !== null;
      }
    );
  }

  function setDetails(showDetails) {
    if (details) details.hidden = !showDetails;
    var customize = part('customize');
    var saveButton = part('save');
    if (customize) customize.hidden = showDetails && !!details;
    if (saveButton) saveButton.hidden = !showDetails;
    root.setAttribute('data-aegis-consent-state', showDetails ? 'details' : 'ask');
  }

  function show(showDetails) {
    if (!root || open) return;
    open = true;
    lastFocus = document.activeElement;
    Array.prototype.forEach.call(checkboxes(), function (box) {
      box.checked = granted(box.getAttribute('data-aegis-consent-category'));
    });
    setDetails(!!showDetails);
    root.removeAttribute('hidden');
    if (reopen) reopen.hidden = true;
    // The button that refuses has the focus, so a reflex key press does not accept
    var first = part('reject-all') || focusable()[0];
    if (first) first.focus();
  }

  function hide() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (root) root.hidden = true;
    var wasOpen = open;
    open = false;
    if (reopen) reopen.hidden = !state;
    if (wasOpen && lastFocus && document.contains(lastFocus)) lastFocus.focus();
  }

  function onRootClick(event) {
    var target = event.target.closest ? event.target.closest('[data-aegis-consent-action]') : null;
    if (!target || !root.contains(target)) return;
    var action = target.getAttribute('data-aegis-consent-action');
    if (action === 'accept-all') {
      save(choicesFrom(true));
    } else if (action === 'reject-all') {
      save(choicesFrom(false));
    } else if (action === 'customize') {
      setDetails(true);
      var box = checkboxes()[0];
      if (box) box.focus();
    } else if (action === 'save') {
      var choices = choicesFrom(false);
      Array.prototype.forEach.call(checkboxes(), function (input) {
        choices[input.getAttribute('data-aegis-consent-category')] = input.checked;
      });
      save(choices);
    }
  }

  function onKeydown(event) {
    if (!open) return;
    // Escape closes the dialog only when a choice was already made
    if (event.key === 'Escape' && state) {
      hide();
      return;
    }
    // A modal keeps the focus inside
    if (event.key === 'Tab' && config.modal) {
      var nodes = focusable();
      if (nodes.length === 0) return;
      var first = nodes[0];
      var last = nodes[nodes.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  function onDocumentClick(event) {
    var target = event.target;
    while (target && target !== document) {
      if (target.hasAttribute && target.hasAttribute('data-aegis-consent-open')) {
        event.preventDefault();
        show(true);
        return;
      }
      target = target.parentNode;
    }
  }

  function onReopenClick() {
    show(true);
  }

  window.AegisConsent = {
    /** Whether the category was accepted */
    has: granted,
    /** The choices: {category: boolean}, or null when nothing was chosen yet */
    get: function () {
      return state ? snapshot() : null;
    },
    /** Opens the settings */
    open: function () {
      show(true);
    },
    /** Removes the listeners and hides the banner (for single-page apps and tests; the cookie stays) */
    destroy: function () {
      document.removeEventListener('click', onDocumentClick);
      document.removeEventListener('keydown', onKeydown);
      if (root) root.removeEventListener('click', onRootClick);
      if (reopen) reopen.removeEventListener('click', onReopenClick);
      hide();
      if (reopen) reopen.hidden = true;
      delete window.AegisConsent;
    },
    /** Calls fn(choices) when the choices change (and once right away when they are known) */
    onChange: function (fn) {
      listeners.push(fn);
      if (state) fn(snapshot());
    },
  };

  function start() {
    root = document.querySelector('[data-aegis-consent-root]');
    reopen = document.querySelector('[data-aegis-consent-reopen]');
    details = root ? root.querySelector('[data-aegis-consent-details]') : null;
    if (root) root.addEventListener('click', onRootClick);
    if (reopen) reopen.addEventListener('click', onReopenClick);
    document.addEventListener('click', onDocumentClick);
    document.addEventListener('keydown', onKeydown);
    if (!root && window.console) {
      console.warn('AegisCMS consent: the theme\'s consent.html has no [data-aegis-consent-root]');
    }
    if (state) {
      activate();
      if (reopen) reopen.hidden = false;
      notify();
    } else if (config.delay > 0) {
      timer = setTimeout(function () {
        timer = null;
        show(false);
      }, config.delay);
    } else {
      show(false);
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
  } else {
    start();
  }
})();
