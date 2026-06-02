ActiveRecord::Schema.define(version: 1) do
  create_table "work_packages", force: :cascade do |t|
    t.string  "subject", null: false
    t.integer "status_id"
  end

  create_table "time_entries", force: :cascade do |t|
    t.float   "hours"
    t.integer "work_package_id"
  end

  create_table "adhoc_things", force: :cascade do |t|
    t.string "label"
  end
end
