document.addEventListener('DOMContentLoaded', () => {
  // Find navigation links by data attributes
  const findLink = (attr) => {
    const link = document.querySelector(`a[${attr}]`);
    return link;
  };

  const fileNext = findLink('data-file-next');
  const filePrev = findLink('data-file-prev');
  const fileFirst = findLink('data-file-first');
  const fileLast = findLink('data-file-last');
  const itemNext = findLink('data-item-next');
  const itemPrev = findLink('data-item-prev');
  const toggleFull = findLink('data-toggle-full');
  const quit = findLink('data-is-quit');

  // Helper to navigate to a link
  const navigateToLink = (link) => {
    if (!link) return;
    if (link.hasAttribute('data-replace-history')) {
      window.location.replace(link.href);
    } else {
      window.location.href = link.href;
    }
  };

  // Track shift key presses for double-tap detection
  let lastShiftPress = 0;
  let shiftPressed = false;
  const SHIFT_DOUBLE_TAP_THRESHOLD = 300; // milliseconds

  document.addEventListener('keydown', function(e) {
    // Double-tap shift: toggle full view
    if (e.key === 'Shift' && !shiftPressed) {
      shiftPressed = true;
      const now = Date.now();
      if (now - lastShiftPress < SHIFT_DOUBLE_TAP_THRESHOLD && toggleFull) {
        e.preventDefault();
        navigateToLink(toggleFull);
        lastShiftPress = 0; // Reset to prevent triple-tap
        return;
      }
      lastShiftPress = now;
      return;
    }

    // File navigation (within item): Arrow Up/Down
    if (e.key === 'ArrowDown' && !e.shiftKey) {
      if (fileNext) {
        e.preventDefault();
        navigateToLink(fileNext);
      }
      return;
    }
    if (e.key === 'ArrowUp' && !e.shiftKey) {
      if (filePrev) {
        e.preventDefault();
        navigateToLink(filePrev);
      }
      return;
    }

    // Item navigation (slideshow): Arrow Left/Right
    if (e.key === 'ArrowRight' && !e.shiftKey) {
      if (itemNext) {
        e.preventDefault();
        navigateToLink(itemNext);
      }
      return;
    }
    if (e.key === 'ArrowLeft' && !e.shiftKey) {
      if (itemPrev) {
        e.preventDefault();
        navigateToLink(itemPrev);
      }
      return;
    }

    // Shift + Arrow Up/Down: first/last file
    if (e.key === 'ArrowUp' && e.shiftKey) {
      if (fileFirst) {
        e.preventDefault();
        navigateToLink(fileFirst);
      }
      return;
    }
    if (e.key === 'ArrowDown' && e.shiftKey) {
      if (fileLast) {
        e.preventDefault();
        navigateToLink(fileLast);
      }
      return;
    }

    // Escape: quit full view
    if (e.key === 'Escape' && quit) {
      e.preventDefault();
      navigateToLink(quit);
      return;
    }
  });

  // Track shift key release
  document.addEventListener('keyup', function(e) {
    if (e.key === 'Shift') {
      shiftPressed = false;
    }
  });

  // Handle clicks on links with data-replace-history
  document.addEventListener('click', (e) => {
    const link = e.target.closest('a[data-replace-history]');
    if (link) {
      e.preventDefault();
      window.location.replace(link.href);
    }
  });
});
