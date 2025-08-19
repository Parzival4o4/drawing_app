/**
 * Represents a single menu item.
 */
export class MenuItem {
  constructor(
    public readonly label: string,
    private readonly onClick: () => void
  ) {}

  render(): HTMLDivElement {
    const div = document.createElement('div');
    div.className = 'popup-menu-item';
    Object.assign(div.style, {
      padding: '4px 12px',
      cursor: 'pointer'
    } as Partial<CSSStyleDeclaration>);
    div.textContent = this.label;
    div.addEventListener('click', (e: MouseEvent) => {
      e.stopPropagation();
      this.onClick();
    });
    return div;
  }
}

/**
 * Represents a separator line between menu items.
 */
export class MenuSeparator {
  render(): HTMLDivElement {
    const sep = document.createElement('div');
    Object.assign(sep.style, {
      height: '1px',
      margin: '4px 0',
      backgroundColor: '#e0e0e0'
    } as Partial<CSSStyleDeclaration>);
    return sep;
  }
}


/**
 * Represents a radio-option group inside the menu.
 */
export class MenuRadioGroup {
  private selectedKey: string;
  private elements: HTMLInputElement[] = [];

  constructor(
    private readonly groupLabel: string,
    private readonly options: Record<string, string>,
    defaultKey: string,
    private readonly onChange: (key: string) => void
  ) {
    this.selectedKey = defaultKey;
  }

  render(): HTMLDivElement {
    const container = document.createElement('div');
    container.className = 'popup-menu-radio-group';
    Object.assign(container.style, { padding: '4px 12px' } as Partial<CSSStyleDeclaration>);

    // Header
    const header = document.createElement('div');
    header.textContent = this.groupLabel;
    header.style.fontWeight = 'bold';
    header.style.marginBottom = '4px';
    container.appendChild(header);

    // Options
    {
      const opts = this.options;
      for (const key in opts) {
        if (Object.prototype.hasOwnProperty.call(opts, key)) {
          const label = opts[key];
          const row = document.createElement('div');
          row.style.display = 'flex';
          row.style.alignItems = 'center';
          row.style.cursor = 'pointer';
          row.style.marginBottom = '2px';

          const input = document.createElement('input');
          input.type = 'radio';
          input.name = `radio-${this.groupLabel}`;
          input.value = key;
          input.checked = key === this.selectedKey;
          input.style.marginRight = '8px';
          this.elements.push(input);

          const lbl = document.createElement('label');
          lbl.textContent = label;

          row.appendChild(input);
          row.appendChild(lbl);
          row.addEventListener('click', (e: MouseEvent) => {
            e.stopPropagation();
            this.select(key);
          });

          container.appendChild(row);
        }
      }
    }
    return container;
  }


  private select(key: string) {
    if (this.selectedKey === key) return;
    this.selectedKey = key;
    this.elements.forEach(el => (el.checked = el.value === key));
    this.onChange(key);
  }

  /** Return currently selected key */
  getSelected(): string {
    return this.selectedKey;
  }
}

/**
 * A context menu that can be shown/hidden and populated with items.
 */
export interface PopupMenu {
  show(x: number, y: number): void;
  hide(): void;
  addItem(item: MenuItem | MenuSeparator): void;
  addItems(...items: Array<MenuItem | MenuSeparator>): void;
}

/**
 * Factory function to create a PopupMenu.
 */
export function createMenu(): PopupMenu {
  // Overlay covers the entire viewport to capture outside clicks
  const overlay = document.createElement('div');
  Object.assign(overlay.style, {
    position: 'fixed',
    top: '0',
    left: '0',
    width: '100%',
    height: '100%',
    background: 'transparent',
    display: 'none',
    zIndex: '999'
  } as Partial<CSSStyleDeclaration>);
  document.body.appendChild(overlay);

  // The menu container
  const menu = document.createElement('div');
  menu.className = 'popup-menu';
  Object.assign(menu.style, {
    position: 'absolute',
    backgroundColor: '#fff',
    border: '1px solid #ccc',
    boxShadow: '0 2px 6px rgba(0,0,0,0.2)',
    display: 'none',
    zIndex: '1000',
    padding: '4px 0',
    borderRadius: '4px',
    minWidth: '150px'
  } as Partial<CSSStyleDeclaration>);
  document.body.appendChild(menu);

  function show(x: number, y: number): void {
    overlay.style.display = 'block';
    menu.style.display = 'block';
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
  }

  function hide(): void {
    overlay.style.display = 'none';
    menu.style.display = 'none';
    // clear menu items if desired
    // menu.innerHTML = '';
  }

  overlay.addEventListener('click', (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    hide();
  });

  function addItem(item: MenuItem | MenuSeparator): void {
    menu.appendChild(item.render());
  }

  function addItems(...items: Array<MenuItem | MenuSeparator>): void {
    items.forEach(addItem);
  }

  return { show, hide, addItem, addItems };
}

// default export for convenience
export default {
  createItem: (label: string, onClick: () => void) => new MenuItem(label, onClick),
  createSeparator: () => new MenuSeparator(),
  createRadioOption: (
    groupLabel: string,
    options: Record<string, string>,
    defaultKey: string,
    onChange: (key: string) => void
  ) => new MenuRadioGroup(groupLabel, options, defaultKey, onChange),
  createMenu
};
