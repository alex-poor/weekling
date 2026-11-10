// Tiny global toast. `showToast(msg)` flashes a message for a couple seconds.
export const toast = $state({ message: '', visible: false });

let timer;
export function showToast(message) {
	toast.message = message;
	toast.visible = true;
	clearTimeout(timer);
	timer = setTimeout(() => {
		toast.visible = false;
	}, 2600);
}
