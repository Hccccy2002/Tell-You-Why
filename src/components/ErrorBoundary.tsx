import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    if (import.meta.env.DEV) {
      console.error(
        "界面渲染失败（不包含用户密钥）",
        error.name,
        info.componentStack,
      );
    }
  }

  render() {
    if (this.state.hasError) {
      return (
        <main className="fatal-error" role="alert">
          <div className="fatal-mark" aria-hidden="true">
            !
          </div>
          <h1>这一页暂时没有正常显示</h1>
          <p>本地知识数据仍然安全。重新载入应用通常可以恢复。</p>
          <button
            className="primary-button"
            onClick={() => window.location.reload()}
          >
            重新载入
          </button>
        </main>
      );
    }
    return this.props.children;
  }
}
