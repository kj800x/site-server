document.addEventListener('DOMContentLoaded', () => {
  const next = [...document.getElementsByTagName("a")].filter((a) => a.hasAttribute("data-is-next"))[0];
  const prev = [...document.getElementsByTagName("a")].filter((a) => a.hasAttribute("data-is-prev"))[0];
  const quit = [...document.getElementsByTagName("a")].filter((a) => a.hasAttribute("data-is-quit"))[0];

  document.addEventListener('keyup', function(e) {
    if (next && e.code == 'ArrowRight') {
      if (next.getAttribute('data-replace-history')) {
        e.preventDefault();
        window.location.replace(next.href);
      } else {
        window.location.href = next.href;
      }
    }
    if (prev && e.code == 'ArrowLeft') {
      if (prev.getAttribute('data-replace-history')) {
        e.preventDefault();
        window.location.replace(prev.href);
      } else {
        window.location.href = prev.href;
      }
    }
    if (quit && e.code == 'Escape') {
      if (quit.getAttribute('data-replace-history')) {
        e.preventDefault();
        window.location.replace(quit.href);
      } else {
        window.location.href = quit.href;
      }
    }
  });
});

document.addEventListener('click', (e) => {
  const link = e.target.closest('a[data-replace-history]');
  if (link) {
    e.preventDefault();
    window.location.replace(link.href);
  }
});
