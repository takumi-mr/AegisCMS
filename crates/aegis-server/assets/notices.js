/*
 * AegisCMS site notices. Served at /_aegis/notices.js when a notice is shown.
 *
 * The notices' HTML comes from the theme's notices.html (a built-in one when the theme has none). A notice
 * the visitor can close has this in it:
 *   [data-aegis-notice="<id>"][data-aegis-notice-rev="<version>"]   the notice
 *   [data-aegis-notice-close]                                       its close button (hidden until this script runs)
 * Without JavaScript the notices simply stay; with it, a closed notice is remembered in localStorage
 * ("aegis_notices": {id: version}) and comes back when the notice is edited (a new version).
 */
(function () {
  'use strict';
  var KEY = 'aegis_notices';
  var MAX_REMEMBERED = 50;
  var items = document.querySelectorAll('[data-aegis-notice]');
  if (!items.length) return;

  var dismissed = {};
  try {
    dismissed = JSON.parse(localStorage.getItem(KEY) || '{}') || {};
  } catch (e) {
    dismissed = {};
  }

  function remember(id, rev) {
    dismissed[id] = rev;
    // keep the file small: forget the oldest ones
    var ids = Object.keys(dismissed);
    while (ids.length > MAX_REMEMBERED) delete dismissed[ids.shift()];
    try {
      localStorage.setItem(KEY, JSON.stringify(dismissed));
    } catch (e) {
      /* storage is blocked: the notice only closes for this page view */
    }
  }

  function remove(item) {
    var parent = item.parentNode;
    parent.removeChild(item);
    // an empty wrapper (the built-in template has one) goes too
    if (parent !== document.body && !parent.querySelector('[data-aegis-notice]')) {
      parent.parentNode.removeChild(parent);
    }
  }

  Array.prototype.forEach.call(items, function (item) {
    var id = item.getAttribute('data-aegis-notice');
    var rev = item.getAttribute('data-aegis-notice-rev');
    var close = item.querySelector('[data-aegis-notice-close]');
    // A notice without a close button always stays
    if (!close) return;
    if (dismissed[id] === rev) {
      remove(item);
      return;
    }
    close.hidden = false;
    close.addEventListener('click', function () {
      remember(id, rev);
      remove(item);
    });
  });
})();
