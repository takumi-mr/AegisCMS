// コメント欄（組み込みの comments.html）の投稿フォームのスクリプト。`/_aegis/comments.js` から読み込む
// （テンプレートにインラインで書くと、CSP の `script-src` に `'unsafe-inline'` が要るため。docs/csp.md の 5）。
// フォーム（`form.aegis-comment-form`）の `data-*` 属性が、投稿先と文言を決める。
(function () {
  var form = document.querySelector("form.aegis-comment-form");
  if (!form) return;
  var status = form.querySelector(".aegis-comment-status");
  var replying = form.querySelector("[data-replying]");
  var KEY = "aegis-comment-author";
  function show(text, kind) { status.textContent = text; status.dataset.kind = kind || ""; }
  function replyTo(id, name) {
    form.elements.parent_id.value = id || "";
    replying.hidden = !id;
    form.querySelector("[data-replying-name]").textContent = name || "";
  }
  try {
    var saved = JSON.parse(localStorage.getItem(KEY) || "{}");
    if (saved.name) form.elements.name.value = saved.name;
    if (saved.email) form.elements.email.value = saved.email;
  } catch (e) {}
  document.querySelectorAll(".aegis-comment-reply").forEach(function (button) {
    button.addEventListener("click", function () {
      replyTo(button.dataset.replyTo, button.dataset.replyName);
      form.scrollIntoView({ behavior: "smooth", block: "start" });
      form.elements.body.focus({ preventScroll: true });
    });
  });
  form.querySelector("[data-cancel-reply]").addEventListener("click", function () { replyTo(); });
  form.addEventListener("submit", function (event) {
    event.preventDefault();
    if (!form.reportValidity()) return;
    var f = form.elements;
    var submit = form.querySelector("button[type=submit]");
    submit.disabled = true;
    show(form.dataset.msgSending);
    fetch(form.dataset.postUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: f.name.value, email: f.email.value, body: f.body.value,
        parent_id: f.parent_id.value || null, notify_replies: f.notify_replies.checked,
        website: f.website.value, captcha_token: captchaToken()
      })
    }).then(function (res) {
      return res.json().catch(function () { return {}; }).then(function (data) {
        if (!res.ok) throw new Error(data.error || form.dataset.msgFailed);
        try { localStorage.setItem(KEY, JSON.stringify({ name: f.name.value, email: f.email.value })); } catch (e) {}
        if (data.status === "approved" && data.comment) {
          location.hash = "comment-" + data.comment.id;
          location.reload();
          return;
        }
        f.body.value = "";
        replyTo();
        show(form.dataset.msgPending);
      });
    }).catch(function (e) { show(e.message, "error"); })
      .finally(function () { submit.disabled = false; resetCaptcha(); });
  });
  // CAPTCHA のトークンは 1 回しか使えない。送信のたびにウィジェットを作り直す
  function captchaToken() {
    var field = form.dataset.captchaField;
    var input = field && form.querySelector('[name="' + field + '"]');
    return input ? input.value : null;
  }
  function resetCaptcha() {
    if (!form.dataset.captchaField) return;
    [window.turnstile, window.hcaptcha, window.grecaptcha].forEach(function (api) {
      try { if (api && api.reset) api.reset(); } catch (e) {}
    });
  }
})();
