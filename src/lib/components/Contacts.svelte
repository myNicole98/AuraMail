<section class="contacts-workspace" aria-label="Contacts">
  <aside class="contacts-sidebar">
    <header class="contacts-head">
      <h2>Contacts</h2>
    </header>
    <p class="contacts-hint">Your address books will appear here.</p>
  </aside>
  <div class="contacts-main"></div>
</section>

<style>
  .contacts-workspace {
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
  }

  .contacts-sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 14px 12px;
    border-right: 1px solid var(--border);
    background: var(--bg-raised);
  }

  .contacts-head h2 {
    margin: 0;
    color: var(--text-strong);
    font-size: 13px;
    font-weight: 650;
  }

  .contacts-hint {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: 12px;
  }

  .contacts-main {
    min-width: 0;
    min-height: 0;
  }
</style>
