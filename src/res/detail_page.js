document.addEventListener('DOMContentLoaded', () => {
  const next = [...document.getElementsByTagName("a")].filter((a) => a.hasAttribute("data-is-next"))[0];
  const prev = [...document.getElementsByTagName("a")].filter((a) => a.hasAttribute("data-is-prev"))[0];

  document.addEventListener('keyup', function(e) {
    if (next && e.code == 'ArrowRight') {
      window.location.href = next.href;
    }
    if (prev && e.code == 'ArrowLeft') {
      window.location.href = prev.href;
    }
  });
});
