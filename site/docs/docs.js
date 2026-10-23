// light the valve tag of the chapter you are reading
const links = [...document.querySelectorAll('.tags a')];
const io = new IntersectionObserver((es) => {
  for (const e of es) if (e.isIntersecting) links.forEach((a) => a.classList.toggle('on', a.hash === '#' + e.target.id));
}, { rootMargin: '-20% 0px -70% 0px' });
document.querySelectorAll('.pages section').forEach((s) => io.observe(s));
