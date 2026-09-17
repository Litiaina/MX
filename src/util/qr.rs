use image::{ImageBuffer, Luma};
use qrcode::QrCode;
use qrcode::types::QrError;


pub async fn generate_qr_image(payload: String) -> Result<ImageBuffer<Luma<u8>, Vec<u8>>, QrError> {
    let code = match QrCode::new(payload.as_bytes()) {
        Ok(c) => c,
        Err(e) => return Err(e),
    };
    let image = code.render::<Luma<u8>>().build();

    Ok(image)
}

