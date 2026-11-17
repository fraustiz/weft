// Saves or restores the general pasteboard, every item and every type, so
// a script that needs the pasteboard can give the user's content back.
//
//   osascript -l JavaScript pasteboard.js save    <dir>
//   osascript -l JavaScript pasteboard.js restore <dir>
//
// <dir> must exist and be private (mode 0700): the content may be a
// secret, such as a password a password manager copied.
ObjC.import('AppKit');

function run(argv) {
    const [mode, dir] = argv;
    const board = $.NSPasteboard.generalPasteboard;
    const files = $.NSFileManager.defaultManager;
    if (mode === 'save') {
        const items = board.pasteboardItems;
        const count = items.isNil() ? 0 : items.count;
        for (let i = 0; i < count; i++) {
            const item = items.objectAtIndex(i);
            const types = [];
            for (let j = 0; j < item.types.count; j++) {
                const type = item.types.objectAtIndex(j);
                const data = item.dataForType(type);
                if (data.isNil()) continue;
                data.writeToFileAtomically(`${dir}/${i}-${types.length}.data`, true);
                types.push(type.js);
            }
            $(types.join('\n')).writeToFileAtomicallyEncodingError(
                `${dir}/${i}.types`, true, $.NSUTF8StringEncoding, null);
        }
        return `saved ${count} item(s)`;
    }
    if (mode === 'restore') {
        const items = [];
        for (let i = 0; files.fileExistsAtPath(`${dir}/${i}.types`); i++) {
            const list = $.NSString.stringWithContentsOfFileEncodingError(
                `${dir}/${i}.types`, $.NSUTF8StringEncoding, null).js;
            const item = $.NSPasteboardItem.alloc.init;
            list.split('\n').filter((type) => type.length > 0).forEach((type, j) => {
                const data = $.NSData.dataWithContentsOfFile(`${dir}/${i}-${j}.data`);
                if (!data.isNil()) item.setDataForType(data, type);
            });
            items.push(item);
        }
        board.clearContents;
        if (items.length > 0) board.writeObjects($(items));
        return `restored ${items.length} item(s)`;
    }
    throw new Error('usage: pasteboard.js save|restore <dir>');
}
