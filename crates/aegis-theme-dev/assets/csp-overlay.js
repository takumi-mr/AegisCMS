(function () {
  var box = null;
  var seen = {};
  var hints = {
    'script-src-elem': 'An inline <script> was blocked. Put it in assets/*.js and load it with <script src>, or output it with {{ inline_script("...") }}.',
    'script-src-attr': 'An on* attribute (onclick, ...) was blocked. Use addEventListener from an external script.',
    'script-src': 'A script was blocked. Use an external file from this theme, or {{ inline_script("...") }}.',
    'style-src-elem': 'An inline <style> or an external stylesheet was blocked. Wrap CSS in {% filter inline_style %}...{% endfilter %}, load assets/*.css, or declare the host in theme.json "csp".style_domains.',
    'font-src': 'A font was blocked. Bundle it under assets/fonts/ or declare the host in theme.json "csp".font_domains.',
    'img-src': 'An image was blocked. Declare its host in theme.json "csp".img_domains.',
    'connect-src': 'A request was blocked (connect-src allows this origin only).',
    'frame-src': 'A frame was blocked (frame-src).'
  };
  function make() {
    box = document.createElement('div');
    box.setAttribute('data-theme-dev-csp', '');
    var s = box.style;
    s.cssText = 'position:fixed;z-index:2147483647;right:12px;bottom:12px;max-width:min(32rem,calc(100vw - 24px));max-height:50vh;overflow:auto;background:#fff7ed;color:#7c2d12;border:1px solid #fb923c;border-radius:8px;padding:10px 12px;font:12px/1.5 system-ui,sans-serif;box-shadow:0 4px 16px rgba(0,0,0,.2)';
    var title = document.createElement('strong');
    title.textContent = 'Blocked by the public site’s CSP (it would not work when published)';
    box.appendChild(title);
    box.appendChild(document.createElement('ul')).style.cssText = 'margin:6px 0 0;padding-left:18px';
    (document.body || document.documentElement).appendChild(box);
  }
  document.addEventListener('securitypolicyviolation', function (e) {
    var d = e.effectiveDirective || e.violatedDirective;
    var what = e.blockedURI && e.blockedURI !== 'inline' ? e.blockedURI : (e.sample || 'inline').slice(0, 60);
    var key = d + '|' + what;
    if (seen[key]) return;
    seen[key] = true;
    if (!box) make();
    var li = document.createElement('li');
    var b = document.createElement('code');
    b.textContent = d + ': ' + what;
    li.appendChild(b);
    var hint = hints[d] || hints[d.replace(/-(elem|attr)$/, '')];
    if (hint) {
      li.appendChild(document.createElement('br'));
      li.appendChild(document.createTextNode(hint));
    }
    box.querySelector('ul').appendChild(li);
  });
})();
