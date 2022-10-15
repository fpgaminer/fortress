// TODO: Remove this module once clipboard handling in GTK/GDK is fixed.
// Currently (Oct 2022) GTK has a bug where it copies extra bytes into the clipboard on macOS.
// So we implement clipboard handling ourselves for now.
use objc::{
	class, msg_send,
	runtime::{Object, BOOL, NO},
	sel, sel_impl,
};
use objc_id::{Id, ShareId};


pub fn copy_text_to_pasteboard(text: &str) -> bool {
	let pasteboard: ShareId<Object> = unsafe { ShareId::from_ptr(msg_send![class!(NSPasteboard), generalPasteboard]) };

	unsafe {
		let _: () = msg_send![pasteboard, clearContents];
		let result: BOOL = msg_send![pasteboard, setString:build_nsstring(text) forType:build_nsstring("public.utf8-plain-text")];

		result != NO
	}
}


fn build_nsstring(text: &str) -> Id<Object> {
	unsafe {
		let nsstring: *mut Object = msg_send![class!(NSString), alloc];
		Id::from_ptr(msg_send![nsstring, initWithBytes:text.as_ptr() length:text.len() encoding:4])
	}
}
