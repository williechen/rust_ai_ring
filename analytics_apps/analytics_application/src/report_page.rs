/// Application 的報表頁面切片；不依賴 HTTP、HTML 或資料庫。
#[derive(Debug, PartialEq, Eq)]
pub struct PageWindow {
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub offset: usize,
}
impl PageWindow {
    pub fn from_count(total: usize, requested_page: usize, requested_size: usize) -> Self {
        let page_size = requested_size.max(1);
        let total_pages = total.saturating_sub(1) / page_size + 1;
        let page = requested_page.clamp(1, total_pages);
        Self {
            page,
            page_size,
            total_pages,
            offset: (page - 1) * page_size,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_out_of_range_pages_clamp() {
        assert_eq!(
            PageWindow::from_count(0, 999, 25),
            PageWindow {
                page: 1,
                page_size: 25,
                total_pages: 1,
                offset: 0
            }
        );
        assert_eq!(
            PageWindow::from_count(27, 999, 25),
            PageWindow {
                page: 2,
                page_size: 25,
                total_pages: 2,
                offset: 25
            }
        );
        assert_eq!(PageWindow::from_count(27, 0, 25).offset, 0);
        assert_eq!(PageWindow::from_count(25, 2, 25).total_pages, 1);
        assert_eq!(
            PageWindow::from_count(usize::MAX, usize::MAX, 1).offset,
            usize::MAX - 1
        );
    }
}
