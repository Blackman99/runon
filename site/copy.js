// copy buttons on every snippet
(() => {
  for (const pre of document.querySelectorAll('pre.snippet')) {
    const b = document.createElement('button');
    b.type = 'button'; b.className = 'copy'; b.textContent = 'Copy';
    b.setAttribute('aria-label', 'Copy this command');
    b.addEventListener('click', async () => {
      const t = pre.querySelector('code').innerText.replace(/\s+#.*$/gm, '').trim();
      try { await navigator.clipboard.writeText(t); b.textContent = 'Copied'; b.classList.add('ok'); }
      catch { b.textContent = 'Select + copy'; }
      setTimeout(() => { b.textContent = 'Copy'; b.classList.remove('ok'); }, 1600);
    });
    pre.append(b);
  }
})();
