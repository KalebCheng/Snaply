import type { Language } from "../types";

const messages = {
  "zh-CN": {
    sessionExpired: "截图会话已失效，请重新截图",
    permission: "Snaply 需要屏幕录制权限，请在系统设置中授权后重启应用。",
    noMonitors: "未检测到可截图的显示器",
    shortcutTaken: "该快捷键已被占用",
    shortcutRegister: "无法注册快捷键",
    commitNoResult: "截图提交没有返回结果",
    pickerClosed: "保存目录选择器意外关闭",
    invalidRequest: "请求格式无效",
    emptyImage: "图片数据为空或已失效",
    invalidPath: "不允许写入该文件路径",
    imageTooLarge: "图片太大，无法复制到剪贴板",
    imageDecodeFailed: "无法解析图片数据",
    monitorMissing: "找不到对应显示器",
    cropFailed: "无法裁切截图",
  },
  en: {
    sessionExpired: "The capture session has expired. Please take another screenshot.",
    permission: "Snaply needs Screen Recording permission. Enable it in System Settings, then restart the app.",
    noMonitors: "No capture-capable displays were found.",
    shortcutTaken: "That shortcut is already in use.",
    shortcutRegister: "Could not register the shortcut.",
    commitNoResult: "The capture commit returned no result.",
    pickerClosed: "The save-folder picker closed unexpectedly.",
    invalidRequest: "The request format is invalid.",
    emptyImage: "The image data is empty or has expired.",
    invalidPath: "Writing to that path is not allowed.",
    imageTooLarge: "The image is too large to place on the clipboard.",
    imageDecodeFailed: "Could not decode the image data.",
    monitorMissing: "The corresponding display could not be found.",
    cropFailed: "Could not crop the screenshot.",
  },
  ru: {
    sessionExpired: "Сеанс снимка устарел. Сделайте новый снимок.",
    permission: "Snaply требуется разрешение на запись экрана. Включите его в системных настройках и перезапустите приложение.",
    noMonitors: "Не найдено ни одного дисплея для снимка.",
    shortcutTaken: "Это сочетание клавиш уже занято.",
    shortcutRegister: "Не удалось зарегистрировать сочетание клавиш.",
    commitNoResult: "Сохранение снимка не вернуло результат.",
    pickerClosed: "Окно выбора папки неожиданно закрылось.",
    invalidRequest: "Неверный формат запроса.",
    emptyImage: "Данные изображения пусты или устарели.",
    invalidPath: "Запись по этому пути запрещена.",
    imageTooLarge: "Изображение слишком велико для буфера обмена.",
    imageDecodeFailed: "Не удалось декодировать изображение.",
    monitorMissing: "Не удалось найти соответствующий дисплей.",
    cropFailed: "Не удалось обрезать снимок.",
  },
} as const;

type ErrorKey = keyof (typeof messages)["zh-CN"];

const patterns: [RegExp, ErrorKey][] = [
  [/截图.*失效|会话.*失效|缓存.*失效|capture.*expired/i, "sessionExpired"],
  [/屏幕录制|Screen Recording|запись экрана/i, "permission"],
  [/未检测到可截图的显示器|no capture-capable displays/i, "noMonitors"],
  [/已被占用|already in use|уже занято/i, "shortcutTaken"],
  [/无法注册|register the shortcut|зарегистрировать/i, "shortcutRegister"],
  [/截图提交没有返回结果|commit returned no result/i, "commitNoResult"],
  [/保存目录选择器|save-folder picker/i, "pickerClosed"],
  [/只接受二进制|request format is invalid|Неверный формат запроса/i, "invalidRequest"],
  [/图片数据为空|截图数据不完整|image data is empty/i, "emptyImage"],
  [/只允许写入截图目录|只允许在截图目录|Writing to that path/i, "invalidPath"],
  [/图片太大|image is too large|слишком велико/i, "imageTooLarge"],
  [/无法解析图片|decode the image/i, "imageDecodeFailed"],
  [/截图尺寸与像素数据不匹配|无法创建.*画布|PNG 编码失败|decode the image/i, "imageDecodeFailed"],
  [/找不到对应显示器|光标所在|corresponding display/i, "monitorMissing"],
  [/无法裁切截图|裁切区域超出|crop the screenshot/i, "cropFailed"],
];

/** Translate known backend/business errors while preserving technical details. */
export function localizeError(error: unknown, language: Language): string {
  const raw = error instanceof Error ? error.message : String(error);
  for (const [pattern, key] of patterns) {
    if (pattern.test(raw)) {
      return messages[language][key];
    }
  }
  return raw;
}
