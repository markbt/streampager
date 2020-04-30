//! Default keymap

keymap! {
    CTRL 'C', 'q' => Quit;
    Escape => Cancel;
    UpArrow, 'k' => ScrollUpLines(1);
    DownArrow, 'j' => ScrollDownLines(1);
    SHIFT UpArrow, ApplicationUpArrow => ScrollUpScreenFraction(4);
    SHIFT DownArrow, ApplicationDownArrow => ScrollDownScreenFraction(4);
    CTRL 'U' => ScrollUpScreenFraction(2);
    CTRL 'D' => ScrollDownScreenFraction(2);
    PageUp, Backspace, 'b', CTRL 'B' => ScrollUpScreenFraction(1);
    PageDown, ' ', 'f', CTRL 'F' => ScrollDownScreenFraction(1);
    Home, 'g' => ScrollToTop;
    End, 'G' => ScrollToBottom;
    LeftArrow => ScrollLeftColumns(4);
    RightArrow => ScrollRightColumns(4);
    SHIFT LeftArrow => ScrollLeftScreenFraction(4);
    SHIFT RightArrow => ScrollRightScreenFraction(4);
    '[' => PreviousFile;
    ']' => NextFile;
    '?' => Help;
    '#' => ToggleLineNumbers;
    '\\' => ToggleLineWrapping;
    ':' => PromptGoToLine;
    '/' => PromptSearchFromStart;
    '>' => PromptSearchForwards;
    '<' => PromptSearchBackwards;
    ',' => PreviousMatch;
    '.' => NextMatch;
    'p' , 'N' => PreviousMatchLine;
    'n' => NextMatchLine;
    '(' => FirstMatch;
    ')' => LastMatch;
}
