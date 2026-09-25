import { useEffect, useState } from "react";
import ConsentGate from "./components/ConsentGate";
import DiscoveryPage from "./pages/DiscoveryPage";
import PreviewPage from "./pages/PreviewPage";
import ReportsPage from "./pages/ReportsPage";
import { consentAccept, consentStatus } from "./api/admin";
import "./App.css";

type Tab = "discovery" | "preview" | "reports";

function App() {
  const [tab, setTab] = useState<Tab>("discovery");
  const [consented, setConsented] = useState<boolean | null>(null);

  useEffect(() => {
    consentStatus()
      .then(setConsented)
      .catch(() => setConsented(false));
  }, []);

  async function handleAccept() {
    try {
      await consentAccept();
      setConsented(true);
    } catch {
      // Still allow UI to proceed if DB write fails in weird edge cases —
      // backend commands will re-check.
      setConsented(true);
    }
  }

  if (consented === null) {
    return (
      <div className="page">
        <p className="muted">加载中…</p>
      </div>
    );
  }

  if (!consented) {
    return <ConsentGate onAccept={() => void handleAccept()} />;
  }

  return (
    <div>
      <nav className="tabs">
        <button
          className={tab === "discovery" ? "active" : ""}
          onClick={() => setTab("discovery")}
        >
          设备发现
        </button>
        <button
          className={tab === "preview" ? "active" : ""}
          onClick={() => setTab("preview")}
        >
          实时预览
        </button>
        <button
          className={tab === "reports" ? "active" : ""}
          onClick={() => setTab("reports")}
        >
          报告与审计
        </button>
      </nav>
      {tab === "discovery" && <DiscoveryPage />}
      {tab === "preview" && <PreviewPage />}
      {tab === "reports" && <ReportsPage />}
    </div>
  );
}

export default App;
