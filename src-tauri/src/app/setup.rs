use crate::{app::window, conf::AppConf, utils};
use enigo::{Enigo, Key, KeyboardControllable};
use log::{error, info};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr;
use std::thread;
use std::time::Duration;
use tauri::Window;
use tauri::{utils::config::WindowUrl, window::WindowBuilder, App, GlobalShortcutManager, Manager};
use winapi::um::winuser::{
  CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, CF_UNICODETEXT,
};
use wry::application::accelerator::Accelerator;

//模拟按键：Ctrl + V，粘贴剪贴板中的文本
fn simulate_paste() {
  let mut enigo = Enigo::new();
  enigo.key_down(Key::Control);
  enigo.key_click(Key::Layout('v'));
  enigo.key_up(Key::Control);

  // 等待按键消息处理
  thread::sleep(Duration::from_millis(100));
  unsafe {
    // 清空剪贴板并关闭
    EmptyClipboard();
    CloseClipboard();
  }
}

// 模拟按键：Ctrl + C，复制选定的文本到剪贴板
fn copy_selected_text() -> Option<String> {
  let mut enigo = Enigo::new();
  enigo.key_down(Key::Control);
  enigo.key_click(Key::Layout('c'));
  enigo.key_up(Key::Control);

  // // 等待按键消息处理
  thread::sleep(Duration::from_millis(100));

  unsafe {
    // 打开剪贴板
    if OpenClipboard(ptr::null_mut()) == 0 {
      println!("Failed to open the clipboard.");
      return None;
    }

    // 获取剪贴板中的文本
    let clipboard_data = GetClipboardData(CF_UNICODETEXT);
    if clipboard_data.is_null() {
      println!("Failed to get clipboard data.");
      CloseClipboard();
      return None;
    }

    // 将剪贴板中的文本转换为 Rust 字符串
    let text_ptr = clipboard_data as *const u16;
    let mut text_length = 0;
    while *text_ptr.offset(text_length) != 0 {
      text_length += 1;
    }

    let text_slice = std::slice::from_raw_parts(text_ptr, text_length as usize);
    let selected_text = OsString::from_wide(text_slice)
      .to_string_lossy()
      .into_owned();

    println!("selected_text: {}", selected_text);

    Some(selected_text)
  }
}

// 聚焦输入框
fn focus_input_textarea(window: &Window, text: &str) {
  let script = format!(
    r#"
    (function() {{
      let textarea = document.querySelector('textarea.resize-none');
      textarea.focus();
    }})();"#,
  );
  window.eval(&script).unwrap();
}

pub fn init(app: &mut App) -> std::result::Result<(), Box<dyn std::error::Error>> {
  info!("stepup");
  let app_conf = AppConf::read();
  let url = app_conf.main_origin.to_string();
  let theme = AppConf::theme_mode();
  let handle = app.app_handle();

  tauri::async_runtime::spawn(async move {
    info!("stepup_tray");
    window::tray_window(&handle);
  });

  if let Some(v) = app_conf.clone().global_shortcut {
    info!("global_shortcut: `{}`", v);
    match v.parse::<Accelerator>() {
      Ok(_) => {
        info!("global_shortcut_register");
        let handle = app.app_handle();
        let mut shortcut = app.global_shortcut_manager();
        shortcut
          .register(&v, move || {
            // 调用获取剪贴板内容的函数
            let selected_text = copy_selected_text().unwrap_or_else(|| "".to_string());
            println!("Selected text: {}", selected_text);

            if let Some(w) = handle.get_window("core") {
              // 显示窗口并将其置于最前端
              w.unminimize().unwrap();
              w.show().unwrap();
              w.set_focus().unwrap();

              println!("Window is shown and focused");

              focus_input_textarea(&w, &selected_text);
              // 等待按键消息处理
              thread::sleep(Duration::from_millis(100));
              // 插入文本到输入框
              simulate_paste();
              thread::sleep(Duration::from_millis(100));
            }
          })
          .unwrap_or_else(|err| {
            error!("global_shortcut_register_error: {}", err);
          });
      }
      Err(err) => {
        error!("global_shortcut_parse_error: {}", err);
      }
    }
  } else {
    info!("global_shortcut_unregister");
  };

  let app_conf2 = app_conf.clone();
  if app_conf.hide_dock_icon {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
  } else {
    let app = app.handle();
    tauri::async_runtime::spawn(async move {
      let link = if app_conf2.main_dashboard {
        "index.html"
      } else {
        &url
      };
      info!("main_window: {}", link);
      let mut main_win = WindowBuilder::new(&app, "core", WindowUrl::App(link.into()))
        .title("ChatGPT")
        .resizable(true)
        .fullscreen(false)
        .inner_size(app_conf2.main_width, app_conf2.main_height)
        .theme(Some(theme))
        .always_on_top(app_conf2.stay_on_top)
        .initialization_script(&utils::user_script())
        .initialization_script(include_str!("../scripts/core.js"))
        .user_agent(&app_conf2.ua_window);

      #[cfg(target_os = "macos")]
      {
        main_win = main_win
          .title_bar_style(app_conf2.clone().titlebar())
          .hidden_title(true);
      }

      if url == "https://chat.openai.com" && !app_conf2.main_dashboard {
        main_win = main_win
          .initialization_script(include_str!("../vendors/floating-ui-core.js"))
          .initialization_script(include_str!("../vendors/floating-ui-dom.js"))
          .initialization_script(include_str!("../vendors/html2canvas.js"))
          .initialization_script(include_str!("../vendors/jspdf.js"))
          .initialization_script(include_str!("../vendors/turndown.js"))
          .initialization_script(include_str!("../vendors/turndown-plugin-gfm.js"))
          .initialization_script(include_str!("../scripts/popup.core.js"))
          .initialization_script(include_str!("../scripts/export.js"))
          .initialization_script(include_str!("../scripts/markdown.export.js"))
          .initialization_script(include_str!("../scripts/cmd.js"))
          .initialization_script(include_str!("../scripts/chat.js"))
      }

      main_win.build().unwrap();
    });
  }

  // auto_update
  let auto_update = app_conf.get_auto_update();
  if auto_update != "disable" {
    info!("run_check_update");
    let app = app.handle();
    utils::run_check_update(app, auto_update == "silent", None);
  }

  Ok(())
}
