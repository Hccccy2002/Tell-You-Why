import { useEffect, useRef } from "react";
import {
  BookOpen,
  Brain,
  FlaskConical,
  Heart,
  Home,
  Library,
  Menu,
  MoreHorizontal,
  Plug,
  Settings,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import { useAutoHideGuard } from "../lib/autoHideGuard";

export type AppView =
  | "home"
  | "interests"
  | "models"
  | "mcp"
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

const menuItems: Array<[AppView, string, LucideIcon]> = [
  ["home", "知识小窗", Home],
  ["study", "学习中心", Brain],
  ["library", "收藏与历史", Library],
  ["knowledge-base", "PDF 知识库", BookOpen],
  ["settings", "设置", Settings],
];

const preferenceItems: Array<[AppView, string, LucideIcon]> = [
  ["interests", "兴趣设置", Heart],
  ["models", "AI 模型", Sparkles],
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
            <Brain size={16} strokeWidth={1.8} aria-hidden="true" />
            <span>学习</span>
          </button>
        ) : null}
        <button
          className="kb-header-button"
          onClick={() => onNavigate("knowledge-base")}
          disabled={navigationLocked}
          aria-label="PDF 知识库"
          aria-current={view === "knowledge-base" ? "page" : undefined}
        >
          <BookOpen size={16} strokeWidth={1.8} aria-hidden="true" />
          <span>PDF</span>
        </button>
        <button
          className="icon-button"
          type="button"
          aria-label={menuOpen ? "关闭菜单" : "打开菜单"}
          aria-expanded={menuOpen}
          aria-controls="main-menu"
          onClick={onMenuToggle}
        >
          <Menu size={18} strokeWidth={1.8} aria-hidden="true" />
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
            {menuItems.map(([target, label, Icon]) => (
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
                <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
                {label}
              </button>
            ))}
            <details
              className="menu-section"
              open={
                view === "interests" || view === "models" ? true : undefined
              }
            >
              <summary>
                <MoreHorizontal
                  size={17}
                  strokeWidth={1.8}
                  aria-hidden="true"
                />
                更多设置
              </summary>
              {preferenceItems.map(([target, label, Icon]) => (
                <button
                  key={target}
                  className={target === view ? "menu-item active" : "menu-item"}
                  onClick={() => onNavigate(target)}
                  aria-current={target === view ? "page" : undefined}
                  disabled={navigationLocked}
                >
                  <Icon size={17} strokeWidth={1.8} aria-hidden="true" />
                  {label}
                </button>
              ))}
            </details>
            <details
              className="developer-menu"
              open={view === "evaluation" || view === "mcp" ? true : undefined}
            >
              <summary>开发者工具</summary>
              <button
                className={view === "mcp" ? "menu-item active" : "menu-item"}
                disabled={navigationLocked}
                onClick={() => onNavigate("mcp")}
              >
                <Plug size={17} strokeWidth={1.8} aria-hidden="true" />
                MCP 服务器
              </button>
              <button
                className={
                  view === "evaluation" ? "menu-item active" : "menu-item"
                }
                disabled={navigationLocked}
                onClick={() => onNavigate("evaluation")}
              >
                <FlaskConical size={17} strokeWidth={1.8} aria-hidden="true" />
                质量评测
              </button>
            </details>
          </nav>
        </div>
      ) : null}
    </header>
  );
}
