-- Backfill compact romaji spellings from existing derived text, without tokenizing books.
UPDATE book_search_fts SET
  title_romaji = (SELECT title_romaji || CASE WHEN instr(title_romaji, ' ') > 0 THEN ' ' || replace(title_romaji, ' ', '') ELSE '' END FROM book_search_documents WHERE book_id = book_search_fts.book_id),
  creator_romaji = (SELECT creator_romaji || CASE WHEN instr(creator_romaji, ' ') > 0 THEN ' ' || replace(creator_romaji, ' ', '') ELSE '' END FROM book_search_documents WHERE book_id = book_search_fts.book_id),
  series_romaji = (SELECT series_romaji || CASE WHEN instr(series_romaji, ' ') > 0 THEN ' ' || replace(series_romaji, ' ', '') ELSE '' END FROM book_search_documents WHERE book_id = book_search_fts.book_id),
  file_name_romaji = (SELECT file_name_romaji || CASE WHEN instr(file_name_romaji, ' ') > 0 THEN ' ' || replace(file_name_romaji, ' ', '') ELSE '' END FROM book_search_documents WHERE book_id = book_search_fts.book_id),
  aliases_romaji = (SELECT aliases_romaji || CASE WHEN instr(aliases_romaji, ' ') > 0 THEN ' ' || replace(aliases_romaji, ' ', '') ELSE '' END FROM book_search_documents WHERE book_id = book_search_fts.book_id);
