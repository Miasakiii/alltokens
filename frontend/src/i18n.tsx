import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';

/**
 * 轻量双语系统（中文 / English）。
 * 用法：const zh = useLang().lang === 'zh';
 *       <h3>{zh ? '标题' : 'Title'}</h3>
 * 英文模式下界面必须是 100% English；中文模式保持现有中文文案。
 */

export type Lang = 'zh' | 'en';

const STORAGE_KEY = 'alltokens-lang';

interface LangContextValue {
  lang: Lang;
  setLang: (lang: Lang) => void;
}

const LangContext = createContext<LangContextValue>({ lang: 'zh', setLang: () => {} });

function getInitialLang(): Lang {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'zh' || stored === 'en') return stored;
  } catch {
    /* localStorage 不可用时回退默认 */
  }
  return 'zh';
}

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(getInitialLang);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch {
      /* ignore */
    }
    document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
  }, [lang]);

  const setLang = useCallback((l: Lang) => setLangState(l), []);

  return <LangContext.Provider value={{ lang, setLang }}>{children}</LangContext.Provider>;
}

export function useLang(): LangContextValue {
  return useContext(LangContext);
}
