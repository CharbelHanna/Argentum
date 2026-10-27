/**
 * The Text mask's box: type what to select, then Enter or Generate Mask.
 * RapidRAW 1.6.7's (MasksPanel.tsx @ 211ef559, the cursor in the box from
 * 59024136); the work is selection.tsx's `findWords`. Ours.
 */

import { Loader2, Send, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { SubMask } from '../../components/panel/right/Masks';
import Button from '../../components/ui/Button';
import Input from '../../components/ui/Input';
import { useProcessStore } from '../../store/useProcessStore';
import { agText } from '../locales';
import { cancelWords, findWords } from './selection';

interface TextWordsProps {
  subMask: SubMask;
  update: (id: string, data: Partial<SubMask>) => void;
}

export default function TextWords({ subMask, update }: TextWordsProps) {
  const { t } = useTranslation();
  const kept: string = subMask.parameters?.agWords ?? '';
  const [words, setWords] = useState(kept);
  const running = useProcessStore((s) => Boolean(s.activeAiTasks[subMask.id]));

  // Another sub-mask selected, or its words changed by Undo.
  useEffect(() => setWords(kept), [subMask.id, kept]);

  const go = () => {
    if (!running && words.trim()) findWords(subMask.id, words, update);
  };

  return (
    <>
      <Input
        autoFocus={!kept}
        disabled={running}
        key={subMask.id}
        onChange={(e) => setWords(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') go();
        }}
        placeholder={agText('textPlaceholder')}
        value={words}
      />
      {running ? (
        <Button
          className="group w-full bg-red-500/15 hover:bg-red-500/25 text-red-400 border border-red-500/30 transition-colors"
          onClick={() => cancelWords(subMask.id)}
        >
          <span className="flex items-center justify-center group-hover:hidden">
            <Loader2 size={16} className="animate-spin mr-2" />
            {t('editor.ai.settings.generating')}
          </span>
          <span className="hidden items-center justify-center group-hover:flex">
            <X size={16} className="mr-2" />
            {t('editor.ai.settings.cancelGeneration')}
          </span>
        </Button>
      ) : (
        <Button className="w-full" disabled={!words.trim()} onClick={go}>
          <Send size={16} />
          <span className="ml-2">{agText('textGenerate')}</span>
        </Button>
      )}
    </>
  );
}
