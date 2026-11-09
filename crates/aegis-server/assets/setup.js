// 初期セットアップ画面（`/setup`）のスクリプト。`/_aegis/setup.js` から読み込む
// （画面にインラインで書くと、CSP の `script-src` に `'unsafe-inline'` が要るため。docs/csp.md の 5）。
(function () {
  const form = document.getElementById('setup-form');
  const errBox = document.getElementById('error');
  const btn = document.getElementById('submit-btn');

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    errBox.style.display = 'none';
    btn.disabled = true;
    btn.innerText = 'Installing...';

    const payload = {
      site_title: document.getElementById('site_title').value.trim(),
      admin_username: document.getElementById('admin_username').value.trim(),
      admin_password: document.getElementById('admin_password').value,
      admin_email: document.getElementById('admin_email').value.trim() || null,
      create_sample_data: document.getElementById('sample_data').checked,
    };

    try {
      const res = await fetch('/api/v1/setup', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      const data = await res.json();
      if (!res.ok) {
        throw new Error(data.error || 'Setup failed');
      }
      btn.innerText = 'Success! Redirecting...';
      setTimeout(() => {
        window.location.href = '/admin/';
      }, 1200);
    } catch (err) {
      errBox.innerText = err.message;
      errBox.style.display = 'block';
      btn.disabled = false;
      btn.innerText = 'Complete Installation';
    }
  });
})();
