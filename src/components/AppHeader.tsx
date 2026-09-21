import { useEffect, useRef } from "react";
import { useAutoHideGuard } from "../lib/autoHideGuard";

export type AppView =
  | "home"
  | "interests"
  | "models"
  | "library"
  | "settings"
  | "knowledge-base"
  | "study"
  | "evaluation";

interface Props {
  onOpenStudy?: () => void;
  view: AppView;
  menuOpen: boolean;
  navigationLocked: boolean;
  onMenuToggle: () => void;
  onNavigate: (view: AppView) => void;
}

const menuItems: Array<[AppView, string, string]> = [
  ["home", "知识小窗", "⌂"],
  ["study", "学习中心", "◈"],
  ["interests", "兴趣设置", "◇"],
  ["models", "模型设置", "◎"],
  ["library", "收藏与历史", "☆"],
  ["knowledge-base", "PDF 知识库", "▤"],
  ["settings", "通用设置", "⚙"],
];

export function AppHeader({
  onOpenStudy,
  view,
  menuOpen,
  navigationLocked,
  onMenuToggle,
  onNavigate,
}: Props) {
  useAutoHideGuard(menuOpen, "main-menu");
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    menuRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, [menuOpen]);

  return (
    <header className="app-header">
      <button
        className="brand-button"
        onClick={() => onNavigate("home")}
        aria-label="返回知识小窗"
        disabled={navigationLocked}
        title={navigationLocked ? "当前操作完成后可返回知识小窗" : undefined}
      >
        <span className="brand-mark" aria-hidden="true">
          T
        </span>
        <span>Tell You Why</span>
      </button>
      <div className="header-actions">
        {onOpenStudy ? (
          <button
            className="kb-header-button"
            onClick={onOpenStudy}
            disabled={navigationLocked}
            aria-label="打开学习中心"
            aria-current={view === "study" ? "page" : undefined}
          >
            学习
          </button>
        ) : null}
        <button
          className="kb-header-button"
          onClick={() => onNavigate("knowledge-base")}
          disabled={navigationLocked}
          aria-label="PDF 知识库"
          aria-current={view === "knowledge-base" ? "page" : undefined}
        >
          PDF
        </button>
        <button
          className="icon-button"
          type="button"
          aria-label={menuOpen ? "关闭菜单" : "打开菜单"}
          aria-expanded={menuOpen}
          aria-controls="main-menu"
          onClick={onMenuToggle}
        >
          <span aria-hidden="true">•••</span>
        </button>
      </div>
      {menuOpen ? (
        <div className="menu-scrim" onClick={onMenuToggle} role="presentation">
          <nav
            id="main-menu"
            className="main-menu"
            ref={menuRef}
            aria-label="主要页面"
            onClick={(event) => event.stopPropagation()}
          >
            {navigationLocked ? (
              <p className="menu-lock-note" role="status">
                当前操作正在进行，完成后可切换页面。
              </p>
            ) : null}
            {menuItems.map(([target, label, icon]) => (
              <button
                key={target}
                className={target === view ? "menu-item active" : "menu-item"}
                onClick={() =>
                  target === "study" && onOpenStudy
                    ? onOpenStudy()
                    : onNavigate(target)
                }
                aria-current={target === view ? "page" : undefined}
                disabled={navigationLocked}
                title={navigationLocked ? "请等待当前操作完成" : undefined}
              >
                <span aria-hidden="true">{icon}</span>
                {label}
              </button>
            ))}
            <details
              className="developer-menu"
              open={view === "evaluation" ? true : undefined}
            >
              <summary>开发者工具</summary>
              <button
                className={
                  view === "evaluation" ? "menu-item active" : "menu-item"
                }
                disabled={navigationLocked}
                onClick={() => onNavigate("evaluation")}
              >
                <span aria-hidden="true">▥</span>质量评测
              </button>
            </details>
          </nav>
        </div>
      ) : null}
    </header>
  );
}
